import { ChevronDown, ChevronRight, Circle, MessageSquare } from 'lucide-react';
import type { CachedSession, Workspace } from '../bridge/types';
import { Menu, type MenuItem } from '../ui/controls';
import { FolderIcon } from '../ui/FolderIcon';
import { RowMenu } from '../ui/RowMenu';
import { copyTreeText, type TreeContext } from './treeActions';
import type { PathNode } from './workspaceTree';

export function WorkspaceNodes({ node, level, ...tree }: TreeContext & { node: PathNode; level: number }) {
  const { selection, actions, collapsed, toggle, searching } = tree;
  const workspace = node.workspace;
  const expanded = !collapsed.has(node.key);
  const current =
    workspace && selection.current?.server === workspace.server && selection.current.path === workspace.path;
  const items: MenuItem[] = workspace
    ? [
        { label: 'Open workspace', action: () => actions.onSelectWorkspace(workspace) },
        { label: 'New session', action: () => actions.onNew(workspace) },
        { label: 'Refresh', action: actions.onRefresh, disabled: actions.refreshing },
        { label: 'Open terminal', action: () => actions.onTerminal(workspace) },
        { label: 'Open in New Window', action: () => actions.onWindow(workspace) },
        { label: 'Copy path', action: () => copyTreeText(node.path, actions) },
        { label: 'Remove workspace…', action: () => actions.onRemoveWorkspace(workspace), danger: true },
      ]
    : [
        { label: expanded ? 'Collapse' : 'Expand', action: () => toggle(node.key) },
        { label: 'Copy path', action: () => copyTreeText(node.path, actions) },
      ];
  return (
    <div className="tree-node">
      <RowMenu items={items}>
        <div
          className={`tree-row ${workspace ? 'workspace-row' : 'directory-row'} ${current && !selection.selected ? 'selected' : ''}`}
          style={{ paddingLeft: 4 + (level - 1) * 14 }}
          data-node-key={node.key}
          data-workspace-path={workspace?.path}
          data-directory-path={workspace ? undefined : node.path}
        >
          <button
            className="tree-disclosure"
            aria-label={workspace ? 'Toggle workspace contents' : 'Toggle directory group'}
            aria-expanded={expanded}
            onClick={() => toggle(node.key)}
          >
            {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </button>
          <button
            className="tree-label"
            role="treeitem"
            aria-level={level}
            aria-expanded={expanded}
            aria-selected={workspace ? Boolean(current && !selection.selected) : undefined}
            title={node.path}
            onClick={() => {
              if (!workspace) toggle(node.key);
            }}
            onKeyDown={(event) => {
              if (workspace && (event.key === 'Enter' || event.key === ' ')) {
                event.preventDefault();
                toggle(node.key, true);
                actions.onSelectWorkspace(workspace);
              }
            }}
            onDoubleClick={() => {
              if (workspace) {
                toggle(node.key, true);
                actions.onSelectWorkspace(workspace);
              }
            }}
          >
            <FolderIcon expanded={expanded} />
            <span className="path-name">{node.label}</span>
          </button>
          <Menu items={items} />
        </div>
      </RowMenu>
      {expanded && (
        <div role="group">
          {node.children.map((child) => (
            <WorkspaceNodes key={child.key} node={child} level={level + 1} {...tree} />
          ))}
          {workspace &&
            node.sessions.map((entry) => (
              <SessionRow key={entry.session.id} entry={entry} workspace={workspace} level={level + 1} {...tree} />
            ))}
          {workspace && !node.sessions.length && !searching && (
            <button
              className="new-session-row"
              style={{ marginLeft: 24 + level * 14 }}
              onClick={() => actions.onNew(workspace)}
            >
              New session
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function SessionRow({
  entry: { session },
  workspace,
  level,
  selection,
  actions,
}: TreeContext & { entry: CachedSession; workspace: Workspace; level: number }) {
  const chat = selection.chats[session.id];
  const items: MenuItem[] = [
    { label: 'Open session', action: () => actions.onSelectSession(workspace.server, session.id, workspace.path) },
    { label: 'Rename…', action: () => actions.onSessionMenu(session.id, 'rename') },
    { label: 'Archive', action: () => actions.onSessionMenu(session.id, 'archive') },
    {
      label: 'Close session',
      action: () => actions.onSessionMenu(session.id, 'close'),
      disabled: !chat || chat.closed,
    },
    { label: 'Copy session ID', action: () => copyTreeText(session.id, actions) },
  ];
  return (
    <div className="tree-node">
      <RowMenu items={items}>
        <div
          className={`tree-row session-row ${selection.selected === session.id ? 'selected' : ''}`}
          style={{ paddingLeft: 20 + (level - 1) * 14 }}
          data-session-id={session.id}
        >
          <button
            className="tree-label"
            role="treeitem"
            aria-level={level}
            aria-selected={selection.selected === session.id}
            title={session.title || session.id}
            onClick={() => actions.onSelectSession(workspace.server, session.id, workspace.path)}
          >
            {chat?.turn ? (
              <span className="pulse" />
            ) : chat?.questions.length ? (
              <Circle className="attention" size={12} fill="currentColor" />
            ) : (
              <MessageSquare size={13} />
            )}
            <span>{session.title || 'New conversation'}</span>
          </button>
          <Menu items={items} />
        </div>
      </RowMenu>
    </div>
  );
}
