import { useState } from 'react';
import { SessionHome } from '../src/app/SessionHome';
import type { Catalog } from '../src/bridge/types';
import { ConnectionTree } from '../src/navigation/ConnectionTree';

const noop = () => {};
const actions = {
  onSelectServer: noop,
  onSelectDirectory: noop,
  onSelectWorkspace: noop,
  onSelectSession: noop,
  onNew: noop,
  onRefresh: noop,
  refreshing: false,
  onAddServer: noop,
  onEditServer: noop,
  onRemoveServer: noop,
  onAddWorkspace: noop,
  onCreateFolder: noop,
  onRemoveWorkspace: noop,
  onSessionMenu: noop,
  onTerminal: noop,
  onWindow: noop,
  report: noop,
};

export function ActivityFixture() {
  const [catalog, setCatalog] = useState<Catalog>(() => {
    const now = Math.floor(Date.now() / 1000);
    return {
      servers: [{ id: 'dev', name: 'dev', endpoint: { host: '192.0.2.1', user: 'root', port: 22 } }],
      workspaces: [{ server_id: 'dev', server: 'dev', path: '/workspace/test', used_at: now }],
      sessions: [
        ['old', now - 3 * 86400],
        ['recent', now - 1],
        ['minutes', now - 120],
        ['unknown', 0],
      ].map(([id, time]) => ({
        server_id: 'dev',
        server: 'dev',
        session: {
          id: String(id),
          title: `${id} · 检查并修复登录流程以及工作区中断恢复 Long session title`,
          cwd: '/workspace/test',
          created_at: now - 4 * 86400,
          updated_at: Number(time),
          archived: false,
          state: 'idle',
        },
      })),
      open_session_ids: [],
      live: [],
    };
  });
  const [collapsed, setCollapsed] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const update = (id: string, age: number) =>
    setCatalog((value) => ({
      ...value,
      sessions: value.sessions.map((entry) =>
        entry.session.id === id
          ? { ...entry, session: { ...entry.session, updated_at: Math.floor(Date.now() / 1000) - age } }
          : entry,
      ),
    }));
  return (
    <div>
      <button onClick={() => update('old', 0)}>Update activity</button>
      <button onClick={() => update('recent', 58)}>Activity boundary</button>
      <div style={{ display: 'flex', gap: 20, height: 450 }}>
        <div id="activity-tree" style={{ width: 240, flex: 'none', background: 'var(--sidebar)' }}>
          <ConnectionTree
            {...actions}
            catalog={catalog}
            hidden={false}
            onHidden={noop}
            collapsed={collapsed}
            onCollapsed={setCollapsed}
            openSessions={[]}
            serverHome={null}
            chats={{}}
            selected={selected}
            onSelectSession={(_, id) => setSelected(id)}
            current={null}
          />
        </div>
        <SessionHome
          catalog={catalog}
          chats={{}}
          target={null}
          ready
          onAddWorkspace={noop}
          onWorkspace={noop}
          onNew={noop}
          onAddServer={noop}
          onOpen={noop}
        />
      </div>
    </div>
  );
}
