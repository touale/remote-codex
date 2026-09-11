import type { SessionOpening } from '../chat/ConversationLoading';
import { useEffect, useRef, useState } from 'react';
import { failure } from '../bridge/client';
import type { CurrentWorkspace } from '../bridge/files';
import { DraftStore } from '../chat/drafts/store';
import { submitDraft, type DraftSubmission } from '../chat/drafts/submit';
import type { useChats } from '../chat/useChats';
import type { useFiles } from '../files/useFiles';
import { useDirectoryFiles } from '../files/useDirectoryFiles';
import { workspaceAncestorKeys } from '../navigation/workspaceTree';
import type { useDialog } from '../ui/useDialog';
import { locationTarget, type NavigationLocation } from './navigationLocation';
import { prepareSession, type Opened } from './sessionOpening';
import type { useApplication } from './useApplication';
import { useWorkspaceAccess } from './useWorkspaceAccess';
export function useNavigation(
  app: Pick<
    ReturnType<typeof useApplication>,
    'catalog' | 'ready' | 'startupTarget' | 'report' | 'finishOperation' | 'setError' | 'changePreferences' | 'refresh'
  >,
  chats: Pick<
    ReturnType<typeof useChats>,
    'store' | 'chats' | 'register' | 'action' | 'update' | 'close' | 'loadHistory'
  >,
  dialog: Pick<ReturnType<typeof useDialog>, 'ask'>,
  files: Pick<ReturnType<typeof useFiles>, 'tabs' | 'retainedContexts'>,
) {
  const [drafts] = useState(() => new DraftStore());
  const [location, setLocation] = useState<NavigationLocation>({ kind: 'home' });
  const activeDraft = useRef<string | null>(null);
  const workspaces = useWorkspaceAccess(app);
  const connect = workspaces.connect;
  const target = locationTarget(location);
  const draftKey = location.kind === 'draft' ? location.key : null;
  const selected = location.kind === 'session' ? location.id : null;
  const serverHome =
    location.kind === 'server' ? (app.catalog.servers.find((s) => s.id === location.serverId) ?? null) : null;
  const currentView = useRef({ target, server: serverHome?.name });
  currentView.current = { target, server: serverHome?.name };
  const browsing = location.kind === 'directory';
  const directoryFiles = useDirectoryFiles(
    browsing ? target : serverHome ? { server: serverHome.name, path: '/' } : null,
    files,
    app.report,
    app.finishOperation,
  );
  const [busy, setBusy] = useState(false);
  // Keep the committed view intact underneath a pending navigation for Back/cancel.
  const [opening, setOpening] = useState<SessionOpening | null>(null);
  const selection = useRef(0);
  const connection = target && !browsing ? workspaces.get(target.server, target.path) : null;
  const workspace =
    location.kind === 'workspace' || location.kind === 'session' ? location.workspace : (connection?.value ?? null);
  const sessions = useRef(new Map<string, Promise<Opened | null>>());
  const activate = (value: CurrentWorkspace, id: string | null) => {
    activeDraft.current = null;
    setBusy(false);
    setLocation(id ? { kind: 'session', id, workspace: value } : { kind: 'workspace', workspace: value });
    const server = app.catalog.servers.find((s) => s.name === value.server);
    const keys = server ? workspaceAncestorKeys(server.id, value.path) : [];
    app.changePreferences((latest) => ({
      selected_workspace: [value.server, value.path],
      collapsed_nodes: latest.collapsed_nodes.filter((k) => !keys.includes(k)),
    }));
  };
  const openWorkspace = async (server: string, path: string) => {
    const epoch = ++selection.current;
    setOpening(null);
    setBusy(true);
    app.setError('');
    try {
      const value = await connect(server, path);
      if (epoch === selection.current) activate(value, null);
      return value;
    } finally {
      if (epoch === selection.current) setBusy(false);
    }
  };
  const allowTakeover = (id: string | null) => app.startupTarget?.kind !== 'session' || app.startupTarget.id !== id;
  const prepare = (server: string, resume: string | null, path: string) =>
    prepareSession(
      { connect, chats, dialog, finishOperation: app.finishOperation, refresh: app.refresh, report: app.report },
      server,
      resume,
      path,
      (phase) =>
        setOpening((current) =>
          current?.id === resume &&
          current.target.server === server &&
          current.target.path === path &&
          current.phase !== 'failed'
            ? { ...current, phase }
            : current,
        ),
      allowTakeover(resume),
    );
  const newSession = (server: string, path: string, key = drafts.ensure({ server, path })) => {
    ++selection.current;
    setOpening(null);
    const target = { server, path };
    activeDraft.current = key;
    setLocation({ kind: 'draft', key, target });
    setBusy(false);
    app.setError('');
    // The draft is usable immediately; file connection failures are shown in its file pane.
    void connect(server, path).catch(() => {});
    const record = app.catalog.servers.find((s) => s.name === server);
    const keys = record ? workspaceAncestorKeys(record.id, path) : [];
    app.changePreferences((latest) => ({
      selected_workspace: [server, path],
      collapsed_nodes: latest.collapsed_nodes.filter((key) => !keys.includes(key)),
    }));
  };
  const openSession = async (server: string, resume: string, path: string) => {
    const epoch = ++selection.current;
    const current = workspaces.get(server, path)?.value;
    const existing = chats.store.get(resume);
    if (current && existing && !existing.closed && existing.historyReady) {
      setOpening(null);
      activate(current, resume);
      setBusy(false);
      return;
    }
    setBusy(true);
    app.setError('');
    setOpening({
      id: resume,
      target: { server, path },
      title:
        existing?.session.title ||
        app.catalog.sessions.find((item) => item.session.id === resume)?.session.title ||
        'Conversation',
      phase: existing && !existing.closed ? 'history' : 'opening',
      error: null,
    });
    const key = JSON.stringify([server, resume, path]);
    let task = sessions.current.get(key);
    if (!task) {
      task = prepare(server, resume, path).finally(() => sessions.current.delete(key));
      sessions.current.set(key, task);
    }
    try {
      const result = await task;
      if (epoch === selection.current) {
        setOpening(null);
        if (result) activate(result.workspace, result.id);
      }
    } catch (error) {
      if (epoch !== selection.current) return;
      const issue = failure(error);
      if (issue.code === 'OPERATION_CANCELLED' || (issue.code === 'SESSION_FOCUSED' && allowTakeover(resume)))
        setOpening(null);
      else setOpening((current) => (current ? { ...current, phase: 'failed', error: issue.message } : current));
    } finally {
      if (epoch === selection.current) setBusy(false);
    }
  };
  const restored = useRef(false);
  useEffect(() => {
    if (!app.ready || restored.current) return;
    restored.current = true;
    const requested = app.startupTarget;
    if (requested?.kind === 'workspace') void openWorkspace(requested.server, requested.path).catch(app.report);
    if (requested?.kind === 'server') {
      const server = app.catalog.servers.find((s) => s.name === requested.server);
      if (server) goHome(server.id);
      else app.report('This server is no longer available.');
    }
    if (requested?.kind === 'session') {
      const entry = app.catalog.sessions.find((s) => s.session.id === requested.id);
      if (entry) void openSession(entry.server, requested.id, entry.session.cwd);
      else app.report('This session is no longer available.');
    }
  });
  const goHome = (serverId?: string) => {
    ++selection.current;
    activeDraft.current = null;
    setOpening(null);
    setBusy(false);
    setLocation(serverId ? { kind: 'server', serverId } : { kind: 'home' });
    app.setError('');
    app.changePreferences((latest) => ({
      selected_workspace: null,
      ...(serverId ? { files_collapsed: false } : {}),
      ...(serverId ? { collapsed_nodes: latest.collapsed_nodes.filter((id) => id !== serverId) } : {}),
    }));
  };
  useEffect(() => {
    if (
      app.ready &&
      ((location.kind === 'server' && !app.catalog.servers.some((s) => s.id === location.serverId)) ||
        (location.kind === 'directory' && !app.catalog.servers.some((s) => s.name === location.target.server)))
    )
      goHome();
  }, [app.ready, app.catalog.servers, location]);
  return {
    opening,
    backFromOpening: () => {
      ++selection.current;
      setOpening(null);
      setBusy(false);
    },
    retryOpening: () => {
      if (opening) void openSession(opening.target.server, opening.id, opening.target.path);
    },
    target,
    drafts,
    draftKey,
    newSession,
    connectFiles: () => {
      if (browsing) return directoryFiles.connect();
      if (!target) return Promise.reject(new Error('Choose a workspace first.'));
      return connect(target.server, target.path);
    },
    submitDraft: (key: string, action: DraftSubmission) => {
      if (activeDraft.current === key) setBusy(true);
      return submitDraft(
        drafts,
        key,
        action,
        (target) => prepare(target.server, null, target.path),
        chats,
        (opened) => {
          if (activeDraft.current === key) activate(opened.workspace, opened.id);
        },
      ).finally(() => {
        if (activeDraft.current === key) setBusy(false);
      });
    },
    workspace,
    fileContext: workspace ? { ...workspace, kind: 'workspace' as const } : directoryFiles.context,
    fileRoot:
      workspace?.path ?? directoryFiles.context?.path ?? target?.path ?? (location.kind === 'server' ? '/' : null),
    fileLoading:
      target && !browsing ? Boolean(connection && !connection.value && connection.error === null) : directoryFiles.busy,
    fileError: target && !browsing ? (connection?.error ?? '') : directoryFiles.error,
    retryFiles: () => {
      if (target && !browsing) void connect(target.server, target.path).catch(() => {});
      else directoryFiles.retry();
    },
    selected,
    busy: opening ? opening.phase !== 'failed' : busy,
    openWorkspace,
    openDirectory: (server: string, path: string) => {
      goHome();
      setLocation({ kind: 'directory', target: { server, path } });
      const record = app.catalog.servers.find((s) => s.name === server);
      const keys = record ? workspaceAncestorKeys(record.id, path) : [];
      app.changePreferences((latest) => ({
        files_collapsed: false,
        collapsed_nodes: latest.collapsed_nodes.filter((k) => !keys.includes(k)),
      }));
    },
    openSession,
    focusSession: (id: string) => {
      const chat = chats.chats[id];
      const saved = app.catalog.sessions.find((s) => s.session.id === id);
      if (chat || saved)
        void openSession(chat?.server ?? saved!.server, id, chat?.session.cwd ?? saved!.session.cwd).catch(app.report);
    },
    forgetWorkspace: (server: string, path?: string) => {
      drafts.forget(server, path);
      const current = currentView.current;
      if (
        (current.target?.server === server && (path === undefined || current.target.path === path)) ||
        (path === undefined && current.server === server)
      )
        goHome();
      directoryFiles.forget(server, path);
      workspaces.forget(server, path);
    },
    clear: () => goHome(),
    selectServer: (serverId: string) => {
      goHome(serverId);
    },
    serverHome,
  };
}
