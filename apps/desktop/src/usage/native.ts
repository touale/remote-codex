import { useSyncExternalStore } from 'react';
import { call, failure } from '../bridge/client';
import type { AccountUsage } from '../bridge/session';
import type { NativeStatus } from '../bridge/types';
import { useVisibleRefresh } from './visible';

interface Snapshot {
  value: AccountUsage | null;
  updatedAt: number | null;
  error: string | null;
  loading: boolean;
}
let snapshot: Snapshot = { value: null, updatedAt: null, error: null, loading: false };
let identity: string | null = null;
let epoch = 0;
let attempted = 0;
let pending: Promise<void> | null = null;
const listeners = new Set<() => void>();
const emit = () => {
  for (const listener of listeners) listener();
};
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
export function invalidateNativeUsage() {
  ++epoch;
  attempted = 0;
  pending = null;
  snapshot = { value: null, updatedAt: null, error: null, loading: false };
  emit();
}
export function acceptNativeIdentity(status: NativeStatus) {
  const next = JSON.stringify([status.program, status.account]);
  if (identity !== null && next !== identity) invalidateNativeUsage();
  identity = next;
}
export function refreshNativeUsage(force = false): Promise<void> {
  if (pending) return pending;
  if (!force && Date.now() - attempted < 60000) return Promise.resolve();
  attempted = Date.now();
  const request = epoch;
  snapshot = { ...snapshot, loading: true };
  emit();
  pending = call('native_usage')
    .then((value) => {
      if (request === epoch) snapshot = { value, updatedAt: Date.now() / 1000, error: null, loading: false };
    })
    .catch((error) => {
      if (request === epoch) snapshot = { ...snapshot, error: failure(error).message, loading: false };
    })
    .finally(() => {
      if (request === epoch) {
        pending = null;
        emit();
      }
    });
  return pending;
}
export function useNativeUsage(active: boolean) {
  const state = useSyncExternalStore(subscribe, () => snapshot);
  useVisibleRefresh(() => {
    void refreshNativeUsage();
  }, active);
  return state;
}
