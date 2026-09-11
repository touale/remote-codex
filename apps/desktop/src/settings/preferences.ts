import { useSyncExternalStore } from 'react';
import { call } from '../bridge/client';
import { defaultAppPreferences, type AppPreferences, type AppPreferencesPatch } from '../bridge/preferences';

let confirmed = defaultAppPreferences;
let snapshot = confirmed;
const pending = new Map<object, AppPreferencesPatch>();
const listeners = new Set<() => void>();
function publish() {
  snapshot = Object.assign({}, confirmed, ...pending.values());
  for (const listener of listeners) listener();
}
export function acceptPreferences(value: AppPreferences) {
  if (value.revision < confirmed.revision) return;
  confirmed = value;
  publish();
}
export async function changeAppPreferences(patch: AppPreferencesPatch) {
  const request = {};
  pending.set(request, patch);
  publish();
  try {
    const value = await call('app_preferences', { patch });
    if (value.revision >= confirmed.revision) confirmed = value;
  } finally {
    pending.delete(request);
    publish();
  }
}
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
export const useAppPreferences = () => useSyncExternalStore(subscribe, () => snapshot);
