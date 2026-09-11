import { useCallback, useEffect, useSyncExternalStore } from 'react';
import { call, failure, listen } from '../../bridge/client';
import type { SessionDefaults } from '../../bridge/types';
import { useVisibleRefresh } from '../../usage/visible';
interface Preview {
  value: SessionDefaults | null;
  error: string | null;
  loading: boolean;
}
interface Entry {
  snapshot: Preview;
  pending?: Promise<void>;
  updated: number;
}
const empty: Preview = { value: null, error: null, loading: false };
const entries = new Map<string, Entry>();
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
export function invalidateSessionDefaults() {
  entries.clear();
  emit();
}
listen((event) => {
  if (event.kind === 'catalog_changed') invalidateSessionDefaults();
});
function refresh(server: string, force = false): Promise<void> {
  const previous = entries.get(server);
  if (previous?.pending) return previous.pending;
  if (!force && previous && Date.now() - previous.updated < 60000) return Promise.resolve();
  const entry: Entry = {
    snapshot: { value: previous?.snapshot.value ?? null, error: null, loading: true },
    updated: Date.now(),
  };
  entries.set(server, entry);
  entry.pending = call('session_defaults', { server })
    .then((value) => {
      entry.snapshot = { value, error: null, loading: false };
    })
    .catch((error) => {
      entry.snapshot = { value: null, error: failure(error).message, loading: false };
    })
    .finally(() => {
      entry.pending = undefined;
      if (entries.get(server) === entry) emit();
    });
  emit();
  return entry.pending;
}
export function useSessionDefaults(server: string, enabled: boolean) {
  const snapshot = useSyncExternalStore(
    subscribe,
    useCallback(() => entries.get(server)?.snapshot ?? empty, [server]),
  );
  useEffect(() => {
    if (enabled && snapshot === empty) void refresh(server);
  }, [server, enabled, snapshot]);
  useVisibleRefresh(() => {
    void refresh(server);
  }, enabled);
  return { ...snapshot, retry: () => refresh(server, true) };
}
