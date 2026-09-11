import { Settings } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { call } from '../bridge/client';
import type { Server, Workspace } from '../bridge/types';
import type { useChats } from '../chat/useChats';
import { FileTree } from '../files/FileTree';
import type { useFileActions } from '../files/useFileActions';
import type { useFiles } from '../files/useFiles';
import { ConnectionTree } from '../navigation/ConnectionTree';
import type { TransferActions } from '../transfers/useTransfers';
import { ResizeHandle } from '../ui/ResizeHandle';
import styles from './Sidebar.module.css';
import type { useApplication } from './useApplication';
import type { useNavigation } from './useNavigation';
import type { useResourceActions } from './useResourceActions';

export function Sidebar({
  app,
  nav,
  fileActions,
  resources,
  chats,
  files,
  fileRevision,
  transfers,
  onSettings,
  onAddServer,
  onEditServer,
  onAddWorkspace,
  onTerminal,
}: {
  app: Pick<
    ReturnType<typeof useApplication>,
    'preferences' | 'catalog' | 'report' | 'refresh' | 'refreshing' | 'changePreferences'
  >;
  nav: Pick<
    ReturnType<typeof useNavigation>,
    | 'fileContext'
    | 'serverHome'
    | 'target'
    | 'opening'
    | 'selected'
    | 'selectServer'
    | 'openWorkspace'
    | 'openSession'
    | 'newSession'
    | 'fileRoot'
    | 'fileLoading'
    | 'fileError'
    | 'retryFiles'
    | 'connectFiles'
  >;
  fileActions: Pick<ReturnType<typeof useFileActions>, 'create' | 'change'>;
  resources: Pick<ReturnType<typeof useResourceActions>, 'removeWorkspace' | 'removeServer'>;
  chats: Pick<ReturnType<typeof useChats>, 'chats' | 'metadata'>;
  files: Pick<ReturnType<typeof useFiles>, 'open'>;
  fileRevision: number;
  transfers: TransferActions;
  onSettings: () => void;
  onAddServer: () => void;
  onEditServer: (server: Server) => void;
  onAddWorkspace: (server: Server) => void;
  onTerminal: (workspace: Workspace) => void;
}) {
  const area = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(500);
  useEffect(() => {
    if (!area.current) return;
    const observer = new ResizeObserver(([entry]) => setHeight(entry.contentRect.height));
    observer.observe(area.current);
    return () => observer.disconnect();
  }, []);
  const p = app.preferences;
  const fileServer = app.catalog.servers.find(
    (server) => server.name === (nav.fileContext?.server ?? nav.serverHome?.name ?? nav.target?.server),
  );
  const bothOpen = !p.workspaces_collapsed && !p.files_collapsed;
  const usable = Math.max(1, height - 3);
  const rows = bothOpen
    ? `minmax(36px, ${p.tree_split}fr) 3px minmax(36px, ${1 - p.tree_split}fr)`
    : p.workspaces_collapsed && p.files_collapsed
      ? '36px 1fr 36px'
      : p.workspaces_collapsed
        ? '36px 0 minmax(36px, 1fr)'
        : 'minmax(36px, 1fr) 0 36px';
  const run = (task: Promise<unknown>) => void task.catch(app.report);
  return (
    <>
      <aside className={styles.sidebar} style={{ width: p.sidebar_width }}>
        <div className={styles.trees} ref={area} style={{ gridTemplateRows: rows }}>
          <div className={styles.area}>
            <ConnectionTree
              catalog={app.catalog}
              chats={chats.chats}
              serverHome={nav.serverHome?.id ?? null}
              onSelectServer={(server) => nav.selectServer(server.id)}
              selected={nav.opening?.id ?? nav.selected}
              current={nav.opening?.target ?? nav.target}
              refreshing={app.refreshing}
              onRefresh={() => run(app.refresh())}
              hidden={p.workspaces_collapsed}
              onHidden={() => app.changePreferences({ workspaces_collapsed: !p.workspaces_collapsed })}
              collapsed={p.collapsed_nodes}
              onCollapsed={(collapsed_nodes) => app.changePreferences({ collapsed_nodes })}
              onSelectWorkspace={(w) => run(nav.openWorkspace(w.server, w.path))}
              onSelectSession={(server, id, path) => run(nav.openSession(server, id, path))}
              onNew={(w) => nav.newSession(w.server, w.path)}
              onAddServer={onAddServer}
              onEditServer={onEditServer}
              onRemoveServer={(s) => run(resources.removeServer(s))}
              onAddWorkspace={onAddWorkspace}
              onRemoveWorkspace={(w) => run(resources.removeWorkspace(w))}
              onSessionMenu={(id, action) =>
                run(chats.metadata(id, action, app.catalog.sessions.find((c) => c.session.id === id)?.session.title))
              }
              onTerminal={onTerminal}
              onWindow={(w) => run(call('new_window', { workspace: { server: w.server, path: w.path } }))}
              report={app.report}
            />
          </div>
          {bothOpen ? (
            <ResizeHandle
              axis="y"
              label="Resize sidebar trees"
              value={p.tree_split * usable}
              min={Math.max(36, usable * 0.15)}
              max={Math.min(usable - 36, usable * 0.85)}
              onChange={(value) => app.changePreferences({ tree_split: value / usable })}
            />
          ) : (
            <div />
          )}
          <div className={styles.area}>
            <FileTree
              onUpload={(parent, folder) => {
                if (nav.fileContext) run(transfers.pick(nav.fileContext, parent, folder));
              }}
              onDownload={(path) => {
                if (nav.fileContext) run(transfers.download(nav.fileContext, path));
              }}
              onDrop={async (parent, grant) => {
                if (nav.fileContext) await transfers.upload(nav.fileContext, parent, grant);
              }}
              report={app.report}
              context={nav.fileContext?.id ?? null}
              server={nav.fileContext?.server ?? nav.serverHome?.name ?? nav.target?.server ?? null}
              loading={nav.fileLoading}
              error={nav.fileError}
              onRetry={nav.retryFiles}
              root={nav.fileRoot}
              onConnect={nav.target && !nav.fileContext ? () => run(nav.connectFiles()) : undefined}
              cacheKey={JSON.stringify([fileServer?.id, nav.fileRoot])}
              refresh={fileRevision}
              collapsed={p.files_collapsed}
              onCollapsed={() => app.changePreferences({ files_collapsed: !p.files_collapsed })}
              onOpen={(path) => {
                if (nav.fileContext) {
                  run(files.open(nav.fileContext.id, path, nav.fileContext.server, nav.fileContext.path));
                  app.changePreferences({ editor_visible: true });
                }
              }}
              onCreate={(directory, parent) =>
                fileActions.create(directory, parent).catch((error) => {
                  app.report(error);
                  return false;
                })
              }
              onRename={(entry) => run(fileActions.change(entry, false))}
              onRemove={(entry) => run(fileActions.change(entry, true))}
            />
          </div>
        </div>
        <div className={styles.footer}>
          <button onClick={onSettings}>
            <Settings size={15} />
            Settings
          </button>
          <small>Local Codex</small>
        </div>
      </aside>
      <ResizeHandle
        axis="x"
        label="Resize sidebar"
        value={p.sidebar_width}
        min={180}
        max={500}
        onChange={(sidebar_width) => app.changePreferences({ sidebar_width })}
      />
    </>
  );
}
