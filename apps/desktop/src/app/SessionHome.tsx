import { ArrowUpRight, Folder, MessageSquare, Plus, SquarePen } from 'lucide-react';
import type { WorkspaceTarget } from '../bridge/files';
import type { CachedSession, Catalog, Server, Workspace } from '../bridge/types';
import type { ChatSummary } from '../chat/store';

import { fullTime } from '../chat/time';
import styles from './SessionHome.module.css';

export function SessionHome({
  catalog,
  chats,
  target,
  server = null,
  onAddWorkspace,
  onWorkspace,
  ready,
  onNew,
  onAddServer,
  onOpen,
}: {
  catalog: Catalog;
  chats: Record<string, ChatSummary>;
  target: WorkspaceTarget | null;
  server?: Server | null;
  onAddWorkspace: (server: Server) => void;
  onWorkspace: (workspace: Workspace) => void;
  ready: boolean;
  onNew: () => void;
  onAddServer: () => void;
  onOpen: (session: CachedSession) => void;
}) {
  const sessions = new Map(catalog.sessions.map((item) => [item.session.id, item]));
  for (const chat of Object.values(chats))
    if (!chat.closed || !sessions.has(chat.session.id))
      sessions.set(chat.session.id, { server_id: '', server: chat.server, session: chat.session });
  const recent = [...sessions.values()]
    .filter(
      (item) =>
        !item.session.archived &&
        (!server || item.server === server.name) &&
        (!target || (item.server === target.server && item.session.cwd === target.path)),
    )
    .sort((a, b) => b.session.updated_at - a.session.updated_at)
    .slice(0, 6);
  const noServers = !catalog.servers.length;
  const workspaces = server ? catalog.workspaces.filter((w) => w.server_id === server.id) : catalog.workspaces;
  const noWorkspaces = !workspaces.length && !target;
  const host = server?.endpoint.host;
  const address = host ? `${host.includes(':') ? `[${host}]` : host}:${server?.endpoint.port ?? 22}` : '';
  const label = noServers ? 'Add server' : noWorkspaces ? 'Add workspace' : 'New session';
  return (
    <section className={styles.home} aria-label={server ? 'Server home' : 'Start working'} data-server-id={server?.id}>
      <div className={styles.inner}>
        <h1>{server ? server.name : target ? 'What would you like to work on?' : 'Welcome to Remote Codex'}</h1>
        <p>
          {server
            ? `${address} · Choose a workspace or continue a conversation.`
            : target
              ? `${target.server} · ${target.path}`
              : noServers
                ? 'Add a server to start working with Codex in your remote environment.'
                : 'Start a new session or pick up where you left off.'}
        </p>
        <div className={styles.homeActions}>
          <button
            className="primary"
            aria-label={label}
            disabled={!ready}
            onClick={noServers ? onAddServer : server && noWorkspaces ? () => onAddWorkspace(server) : onNew}
          >
            {noServers || noWorkspaces ? <Plus size={16} /> : <SquarePen size={16} />}
            {label}
          </button>
          {server && !noWorkspaces && (
            <button onClick={() => onAddWorkspace(server)}>
              <Plus size={16} />
              Add workspace
            </button>
          )}
        </div>
        {server && workspaces.length > 0 && (
          <nav className={styles.recent} aria-label="Server workspaces">
            <h2>Workspaces</h2>
            {workspaces.map((w) => (
              <button key={w.path} onClick={() => onWorkspace(w)} title={w.path}>
                <Folder size={15} />
                <span>
                  <strong>{w.path}</strong>
                </span>
                <ArrowUpRight size={14} />
              </button>
            ))}
          </nav>
        )}
        {recent.length > 0 && (
          <nav className={styles.recent} aria-label="Recent sessions">
            <h2>Recent sessions</h2>
            {recent.map((item) => (
              <button key={item.session.id} onClick={() => onOpen(item)}>
                <MessageSquare size={15} />
                <span>
                  <strong>{item.session.title || 'Untitled session'}</strong>
                  <small>
                    {item.server} · {item.session.cwd}
                  </small>
                </span>
                <time
                  title={fullTime(item.session.updated_at)}
                  dateTime={new Date(item.session.updated_at * 1000).toISOString()}
                >
                  {new Date(item.session.updated_at * 1000).toLocaleDateString([], { month: 'short', day: 'numeric' })}
                </time>
                <ArrowUpRight size={14} />
              </button>
            ))}
          </nav>
        )}
        {!recent.length && !noServers && (
          <small className={styles.hint}>Your sessions will appear here and in Workspaces.</small>
        )}
      </div>
    </section>
  );
}
