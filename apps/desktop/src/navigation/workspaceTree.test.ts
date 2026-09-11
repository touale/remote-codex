import { describe, expect, it } from 'vitest';
import type { CachedSession, Workspace } from '../bridge/types';
import { buildWorkspaceTree, filterWorkspaceTree, workspaceAncestorKeys } from './workspaceTree';

const server = { id: 'dev-id', name: 'dev' };
const workspace = (path: string, owner = server): Workspace => ({
  server_id: owner.id,
  server: owner.name,
  path,
  used_at: 0,
});
const session = (cwd: string, title: string): CachedSession => ({
  server_id: server.id,
  server: server.name,
  session: { id: title, title, cwd, created_at: 0, updated_at: 0, archived: false, state: 'idle' },
});

describe('saved workspace path tree', () => {
  it('shares directory segments without grouping similar names or different servers', () => {
    const other = { id: 'test-id', name: 'test' };
    const paths = ['/workspace/test1', '/workspace/test', '/workspace-old/test', '/workspace/Test'].map((p) =>
      workspace(p),
    );
    paths.push(workspace('/workspace/test', other));
    const roots = buildWorkspaceTree(server, paths);
    expect(roots.map((n) => n.label)).toEqual(['/workspace', '/workspace-old']);
    expect(roots[0].children.map((n) => n.label)).toEqual(['Test', 'test', 'test1']);
    expect(roots[0].children.every((n) => n.workspace?.server_id === server.id)).toBe(true);
    expect(roots[0].key).not.toBe(buildWorkspaceTree(other, paths)[0].key);
  });
  it('compacts single directories, preserves actual parent workspaces and regroups after removal', () => {
    const paths = ['/home/root/projects', '/home/root/projects/test', '/home/root/projects/test/src/lib'].map((p) =>
      workspace(p),
    );
    const roots = buildWorkspaceTree(server, paths, [
      session('/home/root/projects', 'Parent'),
      session('/home/root/projects/test', 'Child'),
    ]);
    expect(roots[0].label).toBe('/home/root');
    const parent = roots[0].children[0];
    expect(parent.label).toBe('projects');
    expect(parent.sessions.map((s) => s.session.title)).toEqual(['Parent']);
    expect(parent.children[0].sessions.map((s) => s.session.title)).toEqual(['Child']);
    expect(parent.children[0].children[0].label).toBe('src');
    const after = buildWorkspaceTree(server, paths.slice(1));
    expect(after[0].label).toBe('/home/root/projects');
    expect(after[0].children[0].workspace?.path).toBe('/home/root/projects/test');
    const rootWorkspace = buildWorkspaceTree(server, [workspace('/'), workspace('/workspace/test')]);
    expect(rootWorkspace[0].workspace?.path).toBe('/');
    expect(rootWorkspace[0].children[0].label).toBe('workspace');
  });
  it('retains compact boundaries and ancestors when searching paths or sessions', () => {
    const paths = ['/workspace/team/a', '/workspace/team/b', '/workspace/other/c'].map((p) => workspace(p));
    const roots = buildWorkspaceTree(server, paths, [
      session(paths[0].path, 'Fix reconnect'),
      session(paths[0].path, 'Unrelated'),
    ]);
    const filtered = filterWorkspaceTree(roots, 'reconnect');
    expect(filtered[0].label).toBe('/workspace');
    expect(filtered[0].children.map((n) => n.label)).toEqual(['team']);
    const leaf = filtered[0].children[0].children[0];
    expect(leaf.sessions.map((s) => s.session.title)).toEqual(['Fix reconnect']);
    expect(filterWorkspaceTree(roots, '/workspace/team')[0].children[0].children).toHaveLength(2);
    expect(filterWorkspaceTree(roots, 'missing')).toEqual([]);
    const reveal = workspaceAncestorKeys(server.id, paths[0].path);
    expect([filtered[0].key, filtered[0].children[0].key, leaf.key].every((key) => reveal.includes(key))).toBe(true);
    expect(reveal).not.toContain(`${server.id}:/workspace/team/b`);
  });
});
