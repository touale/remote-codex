import { Cable, PanelLeft, PanelRight, Terminal as TerminalIcon, X } from 'lucide-react';
import { Tooltip } from 'radix-ui';
import { useState } from 'react';
import type { Server } from '../bridge/types';
import { ConversationLoading } from '../chat/ConversationLoading';
import { DraftChat } from '../chat/drafts/DraftChat';
import { SessionChat } from '../chat/SessionChat';
import { useChats } from '../chat/useChats';
import { useFileActions } from '../files/useFileActions';
import { useFiles } from '../files/useFiles';
import { WorkspaceDialog } from '../navigation/WorkspaceDialog';
import { WorkspacePicker } from '../navigation/WorkspacePicker';
import { Authentication } from '../settings/Authentication';
import { ServerDialog } from '../settings/ServerDialog';
import { SettingsDialog } from '../settings/SettingsDialog';
import { useTerminals } from '../terminal/useTerminals';
import { TransferButton } from '../transfers/TransferButton';
import { useTransfers } from '../transfers/useTransfers';
import { IconButton } from '../ui/controls';
import { useDialog } from '../ui/useDialog';
import styles from './App.module.css';
import { EditorArea, TerminalArea, hasEditorContent } from './ResourcePanels';
import { SessionHome } from './SessionHome';
import { Sidebar } from './Sidebar';
import { StatusBar } from './StatusBar';
import { useApplication } from './useApplication';
import { useNavigation } from './useNavigation';
import { useResourceActions } from './useResourceActions';
import { useWindowActions } from './useWindowActions';

export default function App() {
  const app = useApplication();
  const dialog = useDialog();
  const chats = useChats(app.report, dialog.ask);
  const files = useFiles();
  const transfers = useTransfers(files, dialog.ask, app.report);
  const nav = useNavigation(app, chats, dialog, files);
  const [serverDialog, setServerDialog] = useState<Server | 'new' | null>(null);
  const [workspaceDialog, setWorkspaceDialog] = useState<Server | null>(null);
  const [settings, setSettings] = useState(false);
  const terminals = useTerminals({
    visible: app.preferences.terminal_visible,
    onVisibility: (terminal_visible) => app.changePreferences({ terminal_visible }),
    finishOperation: app.finishOperation,
  });
  const [picker, setPicker] = useState<'session' | 'terminal' | null>(null);
  const [fileRevision, setFileRevision] = useState(0);
  const fileActions = useFileActions(
    nav.fileContext,
    files,
    dialog.ask,
    () => app.changePreferences({ editor_visible: true }),
    () => setFileRevision((v) => v + 1),
  );
  const resources = useResourceActions(app, nav, chats, files, dialog.ask, terminals, fileActions);
  const selected = nav.selected ? chats.chats[nav.selected] : undefined;
  const prefs = app.preferences;
  const conversationTarget = nav.opening?.target ?? nav.target;
  const conversationTitle = nav.opening?.title ?? selected?.session.title ?? 'New conversation';
  app.closeHandler.current = () => {
    void resources.closeWindow();
  };
  const run = (promise: Promise<unknown>) => {
    void promise.catch(app.report);
  };
  const newSession = () => {
    if (conversationTarget) nav.newSession(conversationTarget.server, conversationTarget.path);
    else setPicker('session');
  };
  const toggleTerminal = () => {
    if (nav.opening) return;
    if (nav.workspace) run(terminals.toggle(nav.workspace));
    else if (nav.target) run(nav.connectFiles().then(terminals.open));
    else if (nav.serverHome) run(terminals.toggle({ server: nav.serverHome.name }));
    else setPicker('terminal');
  };
  useWindowActions(app, nav, fileActions, files, newSession, toggleTerminal, setSettings);
  return (
    <Tooltip.Provider delayDuration={400}>
      <main className={styles.app}>
        <header className={styles.titlebar} data-tauri-drag-region>
          <div className={styles.trafficSpace} data-tauri-drag-region />
          <IconButton
            label="Toggle sidebar (⌘B)"
            onClick={() => app.changePreferences({ sidebar_visible: !prefs.sidebar_visible })}
          >
            <PanelLeft size={17} />
          </IconButton>
          <span className={styles.product} data-tauri-drag-region>
            Remote Codex
          </span>
          <div className={styles.dragSpace} data-tauri-drag-region />
          <IconButton
            label={hasEditorContent(nav, files) ? 'Toggle editor' : 'Open a file to show the editor'}
            disabled={!hasEditorContent(nav, files)}
            onClick={() => app.changePreferences({ editor_visible: !prefs.editor_visible })}
          >
            <PanelRight size={17} />
          </IconButton>
          <IconButton label="Toggle terminal (⌘`)" disabled={Boolean(nav.opening)} onClick={toggleTerminal}>
            <TerminalIcon size={17} />
          </IconButton>
        </header>
        <div className={styles.body}>
          {prefs.sidebar_visible && (
            <Sidebar
              app={app}
              nav={nav}
              fileActions={fileActions}
              resources={resources}
              chats={chats}
              files={files}
              fileRevision={fileRevision}
              transfers={transfers}
              onSettings={() => setSettings(true)}
              onAddServer={() => setServerDialog('new')}
              onEditServer={setServerDialog}
              onAddWorkspace={setWorkspaceDialog}
              onTerminal={(w) => run(nav.openWorkspace(w.server, w.path).then(terminals.open))}
            />
          )}
          <div className={styles.workspace}>
            {app.error && (
              <div className={styles.errorBanner} role="alert">
                <Cable size={16} />
                <span>{app.error}</span>
                <button onClick={() => setSettings(true)}>Settings</button>
                <IconButton label="Dismiss error" onClick={() => app.setError('')}>
                  <X size={14} />
                </IconButton>
              </div>
            )}
            <div className={styles.content}>
              <div className={styles.chatColumn}>
                {(nav.opening || nav.selected || nav.draftKey) && (
                  <header className={styles.workspaceToolbar} aria-label="Conversation heading">
                    <div className={styles.context}>
                      <span title={conversationTitle}>{conversationTitle}</span>
                      <small
                        title={conversationTarget ? `${conversationTarget.server} · ${conversationTarget.path}` : ''}
                      >
                        {conversationTarget
                          ? `${conversationTarget.server} / ${conversationTarget.path.split('/').filter(Boolean).at(-1) ?? '/'}`
                          : 'Choose a workspace'}
                      </small>
                    </div>
                  </header>
                )}
                {nav.opening ? (
                  <ConversationLoading opening={nav.opening} onRetry={nav.retryOpening} onBack={nav.backFromOpening} />
                ) : nav.draftKey ? (
                  <DraftChat key={nav.draftKey} draftKey={nav.draftKey} store={nav.drafts} submit={nav.submitDraft} />
                ) : !nav.selected ? (
                  <SessionHome
                    catalog={app.catalog}
                    chats={chats.chats}
                    target={nav.target}
                    server={nav.serverHome}
                    onAddWorkspace={setWorkspaceDialog}
                    onWorkspace={(w) => run(nav.openWorkspace(w.server, w.path))}
                    ready={app.ready}
                    onNew={newSession}
                    onAddServer={() => setServerDialog('new')}
                    onOpen={(item) => run(nav.openSession(item.server, item.session.id, item.session.cwd))}
                  />
                ) : (
                  <SessionChat
                    id={nav.selected}
                    controller={chats}
                    onResume={() => {
                      if (selected) run(nav.openSession(selected.server, selected.session.id, selected.session.cwd));
                    }}
                    onDiff={(change) => {
                      files.setDiff({ ...change, workspace: nav.workspace?.id });
                      app.changePreferences({ editor_visible: true });
                    }}
                    report={app.report}
                  />
                )}
              </div>
              <EditorArea app={app} nav={nav} files={files} fileActions={fileActions} />
            </div>
            <TerminalArea app={app} nav={nav} terminals={terminals} />
            <StatusBar
              transfers={<TransferButton actions={transfers} report={app.report} />}
              drafts={nav.drafts}
              draftKey={nav.draftKey}
              progress={app.progress}
              busy={nav.busy || nav.fileLoading}
              ready={app.ready}
              workspace={Boolean(nav.workspace)}
              serverFiles={nav.fileContext?.kind === 'server'}
              chat={nav.opening ? undefined : selected}
              openingPhase={nav.opening?.phase}
              error={app.error || nav.fileError}
              authenticating={app.prompts.length > 0}
              report={app.report}
            />
          </div>
        </div>
        {serverDialog && (
          <ServerDialog
            server={serverDialog === 'new' ? null : serverDialog}
            onClose={() => setServerDialog(null)}
            onSaved={() => {
              setServerDialog(null);
              run(app.refresh());
            }}
          />
        )}
        {workspaceDialog && (
          <WorkspaceDialog
            server={workspaceDialog}
            onClose={() => setWorkspaceDialog(null)}
            onOpen={nav.openWorkspace}
          />
        )}
        {settings && (
          <SettingsDialog
            preferences={prefs}
            catalog={app.catalog}
            workspace={nav.workspace}
            session={nav.selected}
            onPreferences={app.changePreferences}
            onClose={() => setSettings(false)}
          />
        )}
        {picker && (
          <WorkspacePicker
            catalog={app.catalog}
            server={nav.serverHome}
            onClose={() => setPicker(null)}
            onAdd={(server) => {
              setPicker(null);
              if (server) setWorkspaceDialog(server);
              else setServerDialog('new');
            }}
            onSelect={async (w) => {
              if (picker === 'session') nav.newSession(w.server, w.path);
              else await terminals.open(await nav.openWorkspace(w.server, w.path));
            }}
          />
        )}
        {dialog.dialog}
        {app.prompts[0] && <Authentication key={app.prompts[0].id} request={app.prompts[0]} onAnswer={app.answer} />}
      </main>
    </Tooltip.Provider>
  );
}
