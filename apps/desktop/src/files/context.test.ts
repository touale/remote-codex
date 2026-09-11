import { describe, expect, it } from 'vitest';
import type { Transfer } from '../bridge/files';
import { accept, transferStore } from '../transfers/store';

import { fileUri, remotePath, visibleIn } from './context';

describe('files shared by server and workspace contexts', () => {
  it('recognizes the same file and protects overlapping uploads across roots', () => {
    const root = { id: 'root', server: 'dev', path: '/', kind: 'server' as const };
    const workspace = { ...root, id: 'workspace', path: '/workspace/test', kind: 'workspace' as const };
    const file = { server: 'dev', root: '/', path: 'workspace/test/note.txt' };
    expect(visibleIn(root, file)).toBe(true);
    expect(visibleIn(workspace, file)).toBe(true);
    expect(visibleIn({ ...workspace, path: '/workspace/test2' }, file)).toBe(false);
    expect(visibleIn({ ...root, server: 'other' }, file)).toBe(false);
    expect(remotePath('/', file.path)).toBe(remotePath(workspace.path, 'note.txt'));
    expect(fileUri(file)).toBe(fileUri({ server: 'dev', root: workspace.path, path: 'note.txt' }));
    expect(fileUri({ ...file, path: 'workspace/test/a#b.ts' })).toBe('remote:///dev/workspace/test/a%23b.ts');
    const task: Transfer = {
      id: 'root-upload',
      name: 'note.txt',
      server: 'dev',
      workspace: '/',
      destination: 'workspace/test',
      direction: 'upload',
      status: 'running',
      active: true,
      owned: true,
      bytes: 0,
      total: 10,
      files: 1,
      completed: 0,
      skipped: 0,
      message: null,
      error_code: null,
      conflict: null,
    };
    accept(task);
    expect(transferStore.locked('dev', workspace.path, 'note.txt')).toBe(true);
    expect(transferStore.locked('dev', '/workspace/test2', 'note.txt')).toBe(false);
    expect(transferStore.locked('other', workspace.path, 'note.txt')).toBe(false);
    accept({ ...task, workspace: workspace.path, destination: '' });
    expect(transferStore.locked('dev', '/', file.path)).toBe(true);
    accept({ ...task, status: 'completed', active: false });
    expect(transferStore.locked('dev', '/', file.path)).toBe(false);
  });
});
