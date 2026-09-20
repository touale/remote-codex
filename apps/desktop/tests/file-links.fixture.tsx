import { useEffect, useState } from 'react';
import { EditorArea } from '../src/app/ResourcePanels';
import styles from '../src/app/App.module.css';
import type { useApplication } from '../src/app/useApplication';
import { useNavigation } from '../src/app/useNavigation';
import { SessionChat } from '../src/chat/SessionChat';
import { initialChat } from '../src/chat/state';
import { useChats } from '../src/chat/useChats';
import { useFiles } from '../src/files/useFiles';
import { useDialog } from '../src/ui/useDialog';

const cwd = '/home/fixture/project';
const markdown = [
  '[Absolute](/home/fixture/project/decision.md)',
  '[Relative](./decision.md)',
  '[Summary](tauri://localhost/home/fixture/project/program/5/result_A4/summary.md)',
  '[Encoded](tauri://localhost/home/fixture/project/%E4%B8%AD%E6%96%87%20file.md)',
  '[Outside](../private.md)',
  '[Missing](missing.md)',
  '[Delayed](delayed.md)',
  '[Website](https://example.com/reference)',
].join('\n\n');

export function FileLinksFixture({ app }: { app: ReturnType<typeof useApplication> }) {
  const [error, setError] = useState('');
  const report = (error: unknown) => setError(error instanceof Error ? error.message : String(error));
  const dialog = useDialog();
  const chats = useChats(report, dialog.ask);
  const files = useFiles();
  const nav = useNavigation({ ...app, report, setError, startupTarget: null }, chats, dialog, files);
  useEffect(() => {
    for (const server of ['alpha', 'beta']) {
      const session = { id: server, title: server, cwd, created_at: 1, updated_at: 1, archived: false, state: 'idle' };
      const chat = initialChat(
        session,
        server,
        {
          mode: 'agent',
          model: 'fixture',
          effort: 'medium',
          full_access: false,
          approval_policy: 'on-request',
          reviewer: 'user',
        },
        [],
      );
      chat.historyReady = true;
      chat.messages = [{ id: 'answer', role: 'assistant', text: markdown, complete: true }];
      chats.store.set(server, chat);
    }
  }, [chats.store]);
  return (
    <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
      <div>
        {['alpha', 'beta'].map((server) => (
          <button key={server} onClick={() => void nav.openSession(server, server, cwd).catch(report)}>
            Open {server}
          </button>
        ))}
        <output id="link-session">{nav.selected}</output>
        <output id="link-error">{error}</output>
      </div>
      <div className={styles.content}>
        <div className={styles.chatColumn}>
          <SessionChat
            id={nav.selected}
            controller={chats}
            onResume={() => {}}
            onFreshPlan={async () => {}}
            onDiff={() => {}}
            onOpenFile={nav.openSessionFile}
            report={report}
          />
        </div>
        <EditorArea
          app={{ ...app, report }}
          nav={nav}
          files={files}
          fileActions={{
            save: async () => {},
            close: async (key) => {
              files.close(key);
              return true;
            },
          }}
        />
      </div>
    </div>
  );
}
