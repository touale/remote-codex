import { useSyncExternalStore } from 'react';
import { call, listen } from '../bridge/client';
import type { Transfer } from '../bridge/files';

import { remotePath, within } from '../files/context';
let tasks: Transfer[] = [];
let changedDuringRefresh: Set<string> | null = null;
let refreshing: Promise<void> | null = null;
const subscribers = new Set<() => void>();
const speeds = new Map<string, { bytes: number; time: number; speed: number }>();
const subscribe = (listener: () => void) => {
  subscribers.add(listener);
  return () => {
    subscribers.delete(listener);
  };
};
export function accept(task: Transfer) {
  changedDuringRefresh?.add(task.id);
  const old = speeds.get(task.id);
  const now = performance.now();
  if (!old || now - old.time >= 500)
    speeds.set(task.id, {
      bytes: task.bytes,
      time: now,
      speed: old && task.bytes >= old.bytes ? ((task.bytes - old.bytes) * 1000) / (now - old.time) : 0,
    });
  tasks = [task, ...tasks.filter((t) => t.id !== task.id)];
  for (const subscriber of subscribers) subscriber();
}
listen((event) => {
  if (event.kind === 'transfer') accept(event.transfer);
  if (event.kind === 'transfers_removed') {
    tasks = tasks.filter((t) => !event.ids.includes(t.id));
    for (const id of event.ids) {
      changedDuringRefresh?.add(id);
      speeds.delete(id);
    }
    for (const subscriber of subscribers) subscriber();
  }
});
export const transferStore = {
  subscribe,
  snapshot: () => tasks,
  speed: (id: string) => speeds.get(id)?.speed ?? 0,
  refresh: (): Promise<void> => {
    if (refreshing) return refreshing;
    const changed = new Set<string>();
    changedDuringRefresh = changed;
    refreshing = (async () => {
      const list = await call('transfer_list');
      // Events received during the read are newer than the returned snapshot.
      tasks = [...tasks.filter((t) => changed.has(t.id)), ...list.filter((t) => !changed.has(t.id))];
      for (const subscriber of subscribers) subscriber();
    })().finally(() => {
      changedDuringRefresh = null;
      refreshing = null;
    });
    return refreshing;
  },
  locked: (server: string, root: string, path: string) =>
    tasks.some(
      (t) =>
        t.active &&
        t.direction === 'upload' &&
        t.server === server &&
        within(remotePath(t.workspace, t.destination), remotePath(root, path)),
    ),
};
export const useTransferList = () => useSyncExternalStore(subscribe, transferStore.snapshot);
export const useTransferLock = (server = '', root = '', path = '') =>
  useSyncExternalStore(subscribe, () => transferStore.locked(server, root, path));
