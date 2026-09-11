import type { CachedSession, Server, Workspace } from '../bridge/types';

export interface PathNode {
  key: string;
  path: string;
  label: string;
  workspace: Workspace | null;
  children: PathNode[];
  sessions: CachedSession[];
}
interface Directory {
  path: string;
  workspace: Workspace | null;
  children: Map<string, Directory>;
}
type TreeServer = Pick<Server, 'id' | 'name'>;
const directory = (path: string): Directory => ({ path, workspace: null, children: new Map() });
const compare = (a: Directory, b: Directory) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
const directoryKey = (server: string, path: string) => JSON.stringify(['directory', server, path]);

/** Project saved POSIX workspace paths; no remote directory enumeration is needed. */
export function buildWorkspaceTree(
  server: TreeServer,
  workspaces: Workspace[],
  sessions: CachedSession[] = [],
): PathNode[] {
  const root = directory('/');
  for (const workspace of workspaces) {
    if (workspace.server_id !== server.id) continue;
    let node = root;
    for (const segment of workspace.path.split('/').filter(Boolean)) {
      let child = node.children.get(segment);
      if (!child) {
        child = directory(`${node.path === '/' ? '' : node.path}/${segment}`);
        node.children.set(segment, child);
      }
      node = child;
    }
    node.workspace = workspace;
  }
  const byPath = new Map<string, CachedSession[]>();
  for (const session of sessions) {
    if (session.server !== server.name) continue;
    const entries = byPath.get(session.session.cwd) ?? [];
    entries.push(session);
    byPath.set(session.session.cwd, entries);
  }
  const project = (source: Directory, parent: string | null): PathNode => {
    let node = source;
    while (!node.workspace && node.children.size === 1) {
      const child = [...node.children.values()][0];
      if (child.workspace) break;
      node = child;
    }
    return {
      key: node.workspace ? `${server.id}:${node.path}` : directoryKey(server.id, node.path),
      path: node.path,
      label: parent === null ? node.path : node.path.slice(parent === '/' ? 1 : parent.length + 1),
      workspace: node.workspace,
      children: [...node.children.values()].sort(compare).map((child) => project(child, node.path)),
      sessions: node.workspace ? (byPath.get(node.path) ?? []) : [],
    };
  };
  return root.workspace
    ? [project(root, null)]
    : [...root.children.values()].sort(compare).map((child) => project(child, null));
}

/** Filter the projected tree so searching never changes compact path boundaries. */
export function filterWorkspaceTree(nodes: PathNode[], query: string): PathNode[] {
  const search = query.toLowerCase();
  return nodes.flatMap((node) => {
    if (node.path.toLowerCase().includes(search)) return [node];
    const children = filterWorkspaceTree(node.children, query);
    const sessions = node.sessions.filter((entry) => entry.session.title.toLowerCase().includes(search));
    return children.length || sessions.length ? [{ ...node, children, sessions }] : [];
  });
}

/** Include both roles: a saved workspace may also be an ancestor of another workspace. */
export function workspaceAncestorKeys(server: string, path: string): string[] {
  const keys = [server, `${server}:/`, directoryKey(server, '/')];
  let prefix = '';
  for (const segment of path.split('/').filter(Boolean)) {
    prefix += `/${segment}`;
    keys.push(`${server}:${prefix}`, directoryKey(server, prefix));
  }
  return keys;
}
