import type { Session } from '../bridge/types';

export function activityTimestamp(value: number): number {
  // JavaScript Date's supported range, in seconds.
  return Number.isFinite(value) && value > 0 && value <= 8_640_000_000_000 ? value : 0;
}

export function activityTime(timestamp: number, now: number): string {
  if (!activityTimestamp(timestamp)) return '—';
  const seconds = Math.max(0, Math.floor(now - timestamp));
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86400)}d`;
}

export function compareActivity(a: Session, b: Session): number {
  return activityTimestamp(b.updated_at) - activityTimestamp(a.updated_at) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}
