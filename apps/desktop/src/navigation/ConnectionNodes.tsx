import { ChevronDown, ChevronRight } from 'lucide-react';
import type { Server } from '../bridge/types';
import { Menu, type MenuItem } from '../ui/controls';
import { FolderIcon } from '../ui/FolderIcon';
import { RowMenu } from '../ui/RowMenu';
import { copyTreeText, type TreeContext } from './treeActions';
import { WorkspaceNodes } from './WorkspaceNodes';
import type { PathNode } from './workspaceTree';

export function ServerNode({
  server,
  nodes,
  ...tree
}: TreeContext & {
  server: Server;
  nodes: PathNode[];
}) {
  const { actions, collapsed, toggle, selection } = tree;
  const selected = selection.serverHome === server.id;
  const host = server.endpoint.host.includes(':') ? `[${server.endpoint.host}]` : server.endpoint.host;
  const address = `${host}:${server.endpoint.port ?? 22}`;
  const expanded = !collapsed.has(server.id);
  const items: MenuItem[] = [
    { label: 'Add workspace…', action: () => actions.onAddWorkspace(server) },
    { label: 'Refresh', action: actions.onRefresh, disabled: actions.refreshing },
    { label: 'Server settings…', action: () => actions.onEditServer(server) },
    {
      label: 'Copy address',
      action: () => copyTreeText(`${server.endpoint.user ? `${server.endpoint.user}@` : ''}${address}`, actions),
    },
    { label: 'Remove server…', action: () => actions.onRemoveServer(server), danger: true },
  ];
  return (
    <div className="tree-node">
      <RowMenu items={items}>
        <div className={`tree-row server-row ${selected ? 'selected' : ''}`} data-server-id={server.id}>
          <button
            className="tree-disclosure"
            aria-label="Toggle server contents"
            aria-expanded={expanded}
            onClick={() => toggle(server.id)}
          >
            {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </button>
          <button
            className="tree-label"
            role="treeitem"
            aria-level={1}
            aria-expanded={expanded}
            aria-selected={selected}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                toggle(server.id, true);
                actions.onSelectServer(server);
              }
            }}
            onDoubleClick={() => {
              toggle(server.id, true);
              actions.onSelectServer(server);
            }}
            title={`${server.name} (${address})`}
          >
            <FolderIcon expanded={expanded} />
            <strong>{server.name}</strong>
            <span className="node-detail">({address})</span>
          </button>
          <Menu items={items} />
        </div>
      </RowMenu>
      {expanded && (
        <div role="group">
          {nodes.map((node) => (
            <WorkspaceNodes key={node.key} node={node} level={2} {...tree} />
          ))}
          {nodes.length === 0 && (
            <button className="new-session-row" onClick={() => actions.onAddWorkspace(server)}>
              Add workspace…
            </button>
          )}
        </div>
      )}
    </div>
  );
}
