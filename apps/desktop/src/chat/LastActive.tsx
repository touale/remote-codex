import { useCallback, useSyncExternalStore } from 'react';
import { activityTime, activityTimestamp } from './activity';
import { fullTime } from './time';
import styles from './LastActive.module.css';

const listeners = new Map<() => void, number>();
let timer: ReturnType<typeof setTimeout> | undefined;

function schedule() {
  clearTimeout(timer);
  if (!listeners.size) return;
  const now = Date.now() / 1000;
  const recent = [...listeners.values()].some((timestamp) => now - timestamp < 60);
  timer = setTimeout(
    () => {
      for (const listener of listeners.keys()) listener();
      schedule();
    },
    recent ? 1000 : 60000,
  );
}

/** Only time labels subscribe; ticking never rebuilds the workspace tree or fetches data. */
export function LastActive({ timestamp }: { timestamp: number }) {
  const valid = activityTimestamp(timestamp);
  const subscribe = useCallback(
    (listener: () => void) => {
      if (!valid) return () => {};
      listeners.set(listener, valid);
      schedule();
      return () => {
        listeners.delete(listener);
        schedule();
      };
    },
    [valid],
  );
  const snapshot = useCallback(() => activityTime(timestamp, Date.now() / 1000), [timestamp]);
  const label = useSyncExternalStore(subscribe, snapshot, snapshot);
  const title = valid ? `Last active: ${fullTime(timestamp)}` : 'Last active: unavailable';
  return (
    <time
      className={styles.time}
      title={title}
      aria-label={title}
      dateTime={valid ? new Date(timestamp * 1000).toISOString() : undefined}
    >
      {label}
    </time>
  );
}
