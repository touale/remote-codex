import { expect, it, vi } from 'vitest';
import type { AppEvent } from '../bridge/types';
import type { Transfer } from '../bridge/files';
const bridge = vi.hoisted(() => ({ listener: (_event: AppEvent) => {}, call: vi.fn() }));
vi.mock('../bridge/client', () => ({
  call: bridge.call,
  listen: (listener: typeof bridge.listener) => {
    bridge.listener = listener;
  },
}));
import { accept, transferStore } from './store';

it('finishes a refresh without losing concurrent deletions, progress or saved history', async () => {
  const task: Transfer = {
    id: 'finished',
    name: 'file.txt',
    server: 'dev',
    workspace: '/project',
    direction: 'upload',
    destination: '',
    status: 'completed',
    bytes: 12,
    total: 12,
    files: 1,
    completed: 1,
    skipped: 0,
    message: null,
    error_code: null,
    conflict: null,
    active: false,
    owned: false,
  };
  const running: Transfer = { ...task, id: 'running', status: 'running', active: true, bytes: 0 };
  const saved: Transfer = { ...task, id: 'saved' };
  accept(task);
  let resolve!: (value: Transfer[]) => void;
  bridge.call
    .mockReturnValueOnce(
      new Promise<Transfer[]>((done) => {
        resolve = done;
      }),
    )
    .mockRejectedValue(new Error('A refresh must not restart when progress arrives.'));
  const pending = transferStore.refresh();
  bridge.listener({ kind: 'transfers_removed', ids: [task.id] });
  expect(transferStore.snapshot()).toEqual([]);
  const progress = { ...running, bytes: 8 };
  bridge.listener({ kind: 'transfer', transfer: progress });
  resolve([task, running, saved]);
  await pending;
  expect(transferStore.snapshot()).toEqual([progress, saved]);
});
