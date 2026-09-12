import { ChevronDown, ChevronRight, Plus, RefreshCw, Search } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import type { Catalog } from '../bridge/types';
import { mergeSessions } from '../chat/catalog';
import { IconButton } from '../ui/controls';
import { RowMenu } from '../ui/RowMenu';
import { ServerNode } from './ConnectionNodes';
import { navigateTree } from './keyboard';
import type { TreeActions, TreeSelection } from './treeActions';
import { buildWorkspaceTree, filterWorkspaceTree, workspaceAncestorKeys } from './workspaceTree';

export function ConnectionTree(
  props: TreeActions &
    TreeSelection & {
      catalog: Catalog;
      hidden: boolean;
      onHidden: () => void;
      collapsed: string[];
      onCollapsed: (nodes: string[]) => void;
    },
) {
  const [query, setQuery] = useState('');
  const [searchCollapsed, setSearchCollapsed] = useState<string[]>([]);
  const { catalog, hidden, collapsed } = props;
  const folded = query ? searchCollapsed : collapsed;
  const toggle = (key: string, expand = false) => {
    const next = folded.includes(key) || expand ? folded.filter((id) => id !== key) : [...folded, key];
    if (query) setSearchCollapsed(next);
    else props.onCollapsed(next);
  };
  const sessions = useMemo(
    () => mergeSessions(catalog.sessions, props.chats).filter((item) => !item.session.archived),
    [catalog.sessions, props.chats],
  );
  const trees = useMemo(() => {
    const matches = (value: string) => value.toLowerCase().includes(query.toLowerCase());
    return catalog.servers.flatMap((server) => {
      const all = buildWorkspaceTree(server, catalog.workspaces, sessions);
      const nodes =
        !query || matches(server.name) || matches(server.endpoint.host) ? all : filterWorkspaceTree(all, query);
      return query && !nodes.length && !matches(server.name) && !matches(server.endpoint.host)
        ? []
        : [{ server, nodes }];
    });
  }, [catalog.servers, catalog.workspaces, sessions, query]);
  const currentServer = catalog.servers.find((s) => s.name === props.current?.server)?.id;
  useEffect(() => {
    if (!props.current || !currentServer) return;
    const keys = workspaceAncestorKeys(currentServer, props.current.path);
    setSearchCollapsed((previous) => {
      const next = previous.filter((key) => !keys.includes(key));
      return next.length === previous.length ? previous : next;
    });
  }, [currentServer, props.current?.path, props.selected, collapsed]);
  const backgroundItems = [
    { label: 'Add server…', action: props.onAddServer },
    { label: 'Refresh', action: props.onRefresh, disabled: props.refreshing },
  ];
  return (
    <RowMenu items={backgroundItems} className="tree-context-area">
      <section className="connections" aria-label="Connections and sessions">
        <div className="section-heading">
          <button className="section-title" aria-expanded={!hidden} onClick={props.onHidden}>
            {hidden ? <ChevronRight size={12} /> : <ChevronDown size={12} />}Workspaces
          </button>
          <div className="spacer" />
          <IconButton label="Refresh workspaces" disabled={props.refreshing} onClick={props.onRefresh}>
            <RefreshCw size={14} className={props.refreshing ? 'spinning' : ''} />
          </IconButton>
        </div>
        {!hidden && (
          <>
            <label className="sidebar-search">
              <Search size={14} />
              <input
                aria-label="Search workspaces and sessions"
                placeholder="Search"
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setSearchCollapsed([]);
                }}
              />
            </label>
            <div className="tree-scroll">
              <div role="tree" aria-label="Connections" onKeyDown={navigateTree}>
                {trees.map(({ server, nodes }) => (
                  <ServerNode
                    key={server.id}
                    server={server}
                    nodes={nodes}
                    selection={props}
                    actions={props}
                    collapsed={new Set(folded)}
                    toggle={toggle}
                    searching={Boolean(query)}
                  />
                ))}
                {query && !trees.length && <p className="tree-hint">No matching workspaces or sessions.</p>}
              </div>
              <button className="tree-add-server" onClick={props.onAddServer}>
                <Plus size={14} /> Add server…
              </button>
            </div>
          </>
        )}
      </section>
    </RowMenu>
  );
}
