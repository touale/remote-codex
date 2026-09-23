import { useRef } from 'react';
import { call } from '../bridge/client';
import type { Server, Workspace } from '../bridge/types';
import type { useChats } from '../chat/useChats';
import { remotePath, within } from '../files/context';
import type { useFileActions } from '../files/useFileActions';
import type { useFiles } from '../files/useFiles';
import type { useTerminals } from '../terminal/useTerminals';
import type { Ask } from '../ui/useDialog';
import type { useApplication } from './useApplication';
import type { useNavigation } from './useNavigation';

export function useResourceActions(
  app: Pick<ReturnType<typeof useApplication>, 'preferences' | 'report'>,
  nav: Pick<ReturnType<typeof useNavigation>, 'target' | 'workspace' | 'forgetWorkspace' | 'drafts'>,
  chats: Pick<ReturnType<typeof useChats>, 'chats' | 'close' | 'store'>,
  files: Pick<ReturnType<typeof useFiles>, 'all' | 'allTabs' | 'cancelTransfers'>,
  ask: Ask,
  terminals: Pick<ReturnType<typeof useTerminals>, 'tabs' | 'close'>,
  fileActions: Pick<ReturnType<typeof useFileActions>, 'save' | 'close'>,
) {
  const closing = useRef(false);
  const { tabs, close: closeTerminal } = terminals;
  const removeWorkspace = async (workspace: Workspace) => {
    if (
      !(await ask({
        title: 'Remove workspace?',
        message: `${workspace.server} · ${workspace.path}\n\nThis closes its local sessions and removes Remote Codex session entries. Native history and remote files are preserved.`,
        choices: ['Remove workspace'],
      }))
    )
      return;
    for (const buffer of files
      .allTabs()
      .filter((b) => b.server === workspace.server && within(workspace.path, remotePath(b.root, b.path))))
      if (!(await fileActions.close(buffer.key))) return;
    for (const chat of Object.values(chats.chats).filter(
      (c) => c.server === workspace.server && c.session.cwd === workspace.path && !c.closed,
    ))
      await chats.close(chat.session.id);
    for (const tab of tabs.filter(
      (t) => t.workspace !== null && t.server === workspace.server && t.path === workspace.path,
    ))
      await closeTerminal(tab.id);
    const current =
      nav.target?.server === workspace.server && nav.target.path === workspace.path ? nav.workspace?.id : undefined;
    nav.forgetWorkspace(workspace.server, workspace.path);
    if (current) await call('workspace_close', { id: current });
    await call('workspace_remove', { server: workspace.server, path: workspace.path });
    nav.forgetWorkspace(workspace.server, workspace.path);
  };
  const removeServer = async (server: Server) => {
    if (
      !(await ask({
        title: `Remove ${server.name}?`,
        message:
          'This closes its local sessions, editors and terminals. Remote files and native Codex history are preserved.',
        choices: ['Remove server'],
      }))
    )
      return;
    for (const buffer of files.allTabs().filter((b) => b.server === server.name))
      if (!(await fileActions.close(buffer.key))) return;
    for (const chat of Object.values(chats.chats).filter((c) => c.server === server.name && !c.closed))
      await chats.close(chat.session.id);
    for (const tab of tabs.filter((t) => t.server === server.name)) await closeTerminal(tab.id);
    try {
      await call('server_remove', { name: server.name });
    } finally {
      nav.forgetWorkspace(server.name);
    }
  };
  const closeWindow = async () => {
    if (closing.current) return;
    closing.current = true;
    try {
      if (
        (nav.drafts.hasUnsentText() || Object.keys(chats.chats).some((id) => chats.store.get(id)?.draft.trim())) &&
        !(await ask({
          title: 'Discard unsent messages?',
          message: 'Unsent messages in this window will be lost.',
          choices: ['Discard and close'],
          cancelLabel: 'Keep editing',
        }))
      ) {
        await call('close_window', { cancel: true });
        return;
      }
      await files.cancelTransfers();
      const dirty = files.all().filter((buffer) => buffer.text !== buffer.original);
      if (dirty.length) {
        const answer = await ask({
          title: 'Save changes before closing?',
          message: `${dirty.length} file(s) have unsaved changes.`,
          choices: ['Save all', 'Discard changes'],
        });
        if (!answer) {
          await call('close_window', { cancel: true });
          return;
        }
        if (answer === 'Save all') {
          for (const buffer of dirty) await fileActions.save(buffer.key);
          if (files.all().some((buffer) => buffer.text !== buffer.original))
            throw { message: 'New edits arrived while saving. Review them before closing.' };
        }
      }
      await call('preferences', { value: app.preferences });
      await call('close_window', { cancel: false });
    } catch (error) {
      await call('close_window', { cancel: true });
      app.report(error);
    } finally {
      closing.current = false;
    }
  };
  return { removeWorkspace, removeServer, closeWindow };
}
