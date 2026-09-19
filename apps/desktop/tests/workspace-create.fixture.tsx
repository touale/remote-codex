import { useState } from 'react';
import { Sidebar } from '../src/app/Sidebar';
import type { useApplication } from '../src/app/useApplication';
import { useNavigation } from '../src/app/useNavigation';
import type { Catalog } from '../src/bridge/types';
import { useChats } from '../src/chat/useChats';
import { useFiles } from '../src/files/useFiles';
import { useDialog } from '../src/ui/useDialog';

const noop = () => {};
const done = async () => {};
export function WorkspaceCreateFixture({ app }: { app: ReturnType<typeof useApplication> }) {
  const [catalog, setCatalog] = useState<Catalog>({
    servers: ['beta', 'alpha'].map((name) => ({
      id: name,
      name,
      endpoint: { user: 'dev', host: '192.0.2.1', port: 22 },
    })),
    workspaces: [
      { server: 'alpha', server_id: 'alpha', path: '/workspace/a', used_at: 1 },
      { server: 'alpha', server_id: 'alpha', path: '/workspace/b', used_at: 1 },
      { server: 'beta', server_id: 'beta', path: '/workspace/a', used_at: 1 },
    ],
    sessions: [],
    open_session_ids: [],
    live: [],
  });
  const [error, setError] = useState('');
  const report = (error: unknown) => setError(String(error));
  const fixtureApp = { ...app, catalog, report, setError, startupTarget: null };
  const dialog = useDialog();
  const chats = useChats(report, dialog.ask);
  const files = useFiles();
  const nav = useNavigation(fixtureApp, chats, dialog, files);
  const openWorkspace = async (server: string, path: string) => {
    const workspace = await nav.openWorkspace(server, path);
    // Stand in for the native catalog_changed notification after workspace_open.
    setCatalog((previous) => ({
      ...previous,
      workspaces: [
        ...previous.workspaces.filter((w) => w.server !== server || w.path !== workspace.path),
        { server, server_id: server, path: workspace.path, used_at: 1 },
      ],
    }));
    return workspace;
  };
  return (
    <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
      <Sidebar
        app={fixtureApp}
        nav={{ ...nav, openWorkspace }}
        files={files}
        chats={chats}
        fileActions={{ create: async () => false, change: done, move: done }}
        resources={{ removeWorkspace: done, removeServer: done }}
        fileRevision={0}
        transfers={{ pick: done, download: done, upload: done, remove: done, resume: done, pause: done, cancel: done }}
        onSettings={noop}
        onAddServer={noop}
        onEditServer={noop}
        onAddWorkspace={noop}
        onTerminal={noop}
      />
      <div>
        <button onClick={() => void openWorkspace('beta', '/workspace/a').catch(report)}>Open beta workspace</button>
        <output id="created-workspace">{nav.target ? `${nav.target.server} · ${nav.target.path}` : ''}</output>
        <output id="workspace-create-error">{error}</output>
      </div>
    </div>
  );
}
