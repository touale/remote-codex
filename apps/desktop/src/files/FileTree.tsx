import { useEffect, useRef, useState } from 'react';
import { MoveDialog } from './MoveDialog';
import { useFileMoveDrag } from './useFileMoveDrag';
import type { Granted } from '../bridge/files';
import { useFileDrop } from '../transfers/useFileDrop';

import { ChevronDown, ChevronRight, File, FilePlus2, FolderPlus, RefreshCw } from 'lucide-react';
import type { Entry } from '../bridge/types';
import { navigateTree } from '../navigation/keyboard';
import { Empty, ErrorText, IconButton, Menu, type MenuItem } from '../ui/controls';
import { FolderIcon } from '../ui/FolderIcon';
import { RowMenu } from '../ui/RowMenu';
import { FileTreeSkeleton } from './FileLoading';
import { useFileTree } from './useFileTree';

export function FileTree({
  context,
  server,
  loading,
  error: connectionError,
  onRetry,
  collapsed,
  onCollapsed,
  root,
  refresh,
  cacheKey,
  onOpen,
  onConnect,
  onCreate,
  onRename,
  onMove,
  onRemove,
  onUpload,
  onDownload,
  onDrop,
  report,
}: {
  context: string | null;
  server: string | null;
  loading: boolean;
  error: string;
  onRetry: () => void;
  collapsed: boolean;
  onCollapsed: () => void;
  root: string | null;
  refresh: number;
  cacheKey: string;
  onOpen: (path: string) => void;
  onConnect?: () => void;
  onCreate: (directory: boolean, parent: string) => Promise<boolean>;
  onRename: (entry: Entry) => void;
  onMove: (entry: Entry, parent: string) => Promise<void>;
  onRemove: (entry: Entry) => void;
  onUpload: (parent: string, folder: boolean) => void;
  onDownload: (path: string) => void;
  onDrop: (parent: string, grant: Granted) => Promise<void>;
  report: (error: unknown) => void;
}) {
  const drop = useFileDrop(Boolean(context), onDrop, report);
  const { pages, expanded, states, load, reload, revealed, toggle, reveal } = useFileTree(
    context,
    cacheKey,
    refresh,
    root,
    server,
  );
  const [moving, setMoving] = useState<Entry | null>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const drag = useFileMoveDrag(context, onMove, report);
  useEffect(() => setMoving(null), [context]);
  useEffect(() => {
    if (revealed)
      scroll.current
        ?.querySelector<HTMLElement>(`[data-path="${CSS.escape(revealed)}"]`)
        ?.scrollIntoView({ block: 'nearest' });
  }, [revealed, pages]);
  const create = async (directory: boolean, parent = '') => {
    if (await onCreate(directory, parent)) await reveal(parent);
  };
  const creationItems = (parent: string): MenuItem[] => [
    { label: 'New file…', action: () => void create(false, parent), disabled: !context },
    { label: 'New folder…', action: () => void create(true, parent), disabled: !context },
  ];
  const uploadItems = (parent: string): MenuItem[] => [
    { label: 'Upload files…', action: () => onUpload(parent, false), disabled: !context },
    { label: 'Upload folder…', action: () => onUpload(parent, true), disabled: !context },
  ];
  const backgroundItems: MenuItem[] = [
    ...(onConnect ? [{ label: 'Connect files', action: onConnect }] : []),
    ...creationItems(''),
    ...uploadItems(''),
    { label: 'Refresh', action: reload, disabled: !context },
  ];
  const branch = (path: string, depth: number): React.ReactNode => {
    const state = states[path];
    return (
      <>
        {state?.status === 'failed' && (
          <div className="tree-load-error" style={{ paddingLeft: 8 + depth * 14 }}>
            <ErrorText message={state.error} />
            <button className="tree-retry" onClick={() => void load(path)}>
              Retry
            </button>
          </div>
        )}
        {!pages[path] && state?.status !== 'failed' && <FileTreeSkeleton depth={depth} path={path} />}
        {rows(path, depth)}
        {pages[path]?.entries.length === 0 && state?.status === 'ready' && (
          <div className="tree-hint">This directory is empty.</div>
        )}
        {pages[path]?.truncated && (
          <div className="tree-hint">Directory listing is limited. Open a subfolder to see more.</div>
        )}
      </>
    );
  };
  const rows = (path: string, depth: number): React.ReactNode =>
    pages[path]?.entries.map((entry) => {
      const parent = entry.directory ? entry.path : entry.path.split('/').slice(0, -1).join('/');
      const items: MenuItem[] = [
        { label: 'Download…', action: () => onDownload(entry.path), disabled: entry.symlink },
        ...uploadItems(parent),
        ...(entry.directory ? creationItems(entry.path) : []),
        { label: 'Move to…', action: () => setMoving(entry) },
        { label: 'Rename…', action: () => onRename(entry) },
        { label: 'Delete…', action: () => onRemove(entry), danger: true },
      ];
      return (
        <div className="tree-node" key={entry.path}>
          <RowMenu items={items}>
            <div
              className="tree-row file-row"
              data-path={entry.path}
              data-revealed={entry.path === revealed}
              draggable={!!context}
              onDragStart={(event) => drag.start(event, entry)}
              onDragEnd={drag.end}
              data-move-target={entry.directory && !entry.symlink ? entry.path : undefined}
              data-upload-target={parent}
              data-drop-target={entry.directory && (drop.target === entry.path || drag.target === entry.path)}
              style={{ paddingLeft: 8 + depth * 14 }}
            >
              <button
                className="tree-label"
                role="treeitem"
                title={entry.path}
                aria-expanded={entry.directory ? expanded.has(entry.path) : undefined}
                onClick={() => {
                  if (entry.directory) void toggle(entry.path);
                  else onOpen(entry.path);
                }}
              >
                {entry.directory ? (
                  expanded.has(entry.path) ? (
                    <ChevronDown size={12} />
                  ) : (
                    <ChevronRight size={12} />
                  )
                ) : (
                  <span className="tree-indent" />
                )}
                {entry.directory ? <FolderIcon expanded={expanded.has(entry.path)} /> : <File size={14} />}
                <span>{entry.name}</span>
                {states[entry.path]?.status === 'loading' && pages[entry.path] && expanded.has(entry.path) && (
                  <RefreshCw className="spinning" size={12} aria-label="Refreshing directory" />
                )}
              </button>
              <Menu items={items} />
            </div>
          </RowMenu>
          {entry.directory && expanded.has(entry.path) && branch(entry.path, depth + 1)}
        </div>
      );
    });
  return (
    <>
      {moving && context && root && (
        <MoveDialog context={context} root={root} entry={moving} onMove={onMove} onClose={() => setMoving(null)} />
      )}
      <RowMenu items={backgroundItems} className="tree-context-area">
        <section
          ref={drop.area}
          className="file-tree"
          aria-label="Remote files"
          data-server={server}
          data-dropping={drop.target !== null || drag.target !== null}
        >
          {drag.target !== null && (
            <div className="file-drop-hint" role="status">
              Move to {drag.target || root}
            </div>
          )}
          {drop.target !== null && (
            <div className="file-drop-hint" role="status">
              {context ? `Upload to ${drop.target || root || 'workspace'}` : 'Connect files to upload'}
            </div>
          )}
          <div className="section-heading">
            <button className="section-title" aria-expanded={!collapsed} onClick={onCollapsed}>
              {collapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}Files
            </button>
            <div className="spacer" />
            <IconButton label="New file" disabled={!context} onClick={() => void create(false)}>
              <FilePlus2 size={14} />
            </IconButton>
            <IconButton label="New folder" disabled={!context} onClick={() => void create(true)}>
              <FolderPlus size={14} />
            </IconButton>
            <IconButton label="Refresh files" disabled={!context} onClick={reload}>
              <RefreshCw size={14} className={states['']?.status === 'loading' ? 'spinning' : undefined} />
            </IconButton>
          </div>
          {!collapsed && (
            <>
              <div className="workspace-path" title={root ?? ''}>
                {root ?? 'No location selected'}
              </div>
              <div
                ref={scroll}
                className="tree-scroll"
                onDragOver={drag.over}
                onDragLeave={drag.leave}
                onDrop={drag.drop}
                role="tree"
                aria-label="Files"
                onKeyDown={navigateTree}
              >
                <ErrorText message={connectionError} />
                {connectionError && (
                  <button className="tree-retry" onClick={onRetry}>
                    Retry
                  </button>
                )}
                {loading && !context && <FileTreeSkeleton />}
                {context ? (
                  branch('', 0)
                ) : loading || connectionError ? null : (
                  <Empty
                    title={onConnect ? 'Files are not connected' : 'Choose a server or workspace'}
                    detail={onConnect ? 'Connect to browse this workspace.' : 'Its remote files appear here.'}
                  >
                    {onConnect && <button onClick={onConnect}>Connect files</button>}
                  </Empty>
                )}
              </div>
            </>
          )}
        </section>
      </RowMenu>
    </>
  );
}
