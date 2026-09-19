import type { Transfer } from '../bridge/files';

export const finished = (task: Transfer) => task.status === 'completed' || task.status === 'cancelled';

export function transferSummary(tasks: Transfer[], speed: (id: string) => number) {
  const pending = tasks.filter((task) => !finished(task));
  const active = pending.filter(
    (task) => task.active && ['queued', 'preparing', 'running', 'reconnecting'].includes(task.status),
  );
  const running = active.filter((task) => task.status === 'running');
  const attention = pending.filter(
    (task) => task.status === 'conflict' || (task.status === 'paused' && task.error_code),
  ).length;
  const paused = pending.filter((task) => task.status === 'paused' && !task.error_code).length;
  const bytes = active.reduce((sum, task) => sum + task.bytes, 0);
  const total = active.reduce((sum, task) => sum + task.total, 0);
  const unknown = active.some((task) => task.total <= 0 || ['preparing', 'queued'].includes(task.status));
  const direction =
    active.length && active.every((task) => task.direction === active[0].direction) ? active[0].direction : null;
  let label = 'Transfers';
  if (active.length === 1) {
    const status = active[0].status;
    if (status === 'running') label = direction === 'upload' ? 'Uploading' : 'Downloading';
    else if (status === 'reconnecting') label = 'Reconnecting…';
    else if (status === 'queued') label = 'Queued';
    else label = 'Preparing…';
  } else if (active.length > 1) {
    label = `${active.length} transfers`;
    if (!running.length) {
      if (active.every((task) => task.status === 'reconnecting')) label += ' · Reconnecting…';
      else if (active.every((task) => task.status === 'queued')) label += ' · Queued';
      else label += ' · Preparing…';
    }
  } else if (pending.length) {
    if (attention) label = 'Needs attention';
    else if (paused === pending.length) label = 'Paused';
    else if (paused) label = 'Waiting';
    else label = 'Queued';
    if (pending.length > 1) label = `${pending.length} transfers · ${label}`;
  }
  return {
    label,
    direction,
    bytes,
    total,
    attention,
    active: active.length,
    waiting: pending.length - active.length,
    percent: active.length && !unknown && total > 0 ? Math.min(100, Math.floor((bytes / total) * 100)) : null,
    speed: running.length ? running.reduce((sum, task) => sum + speed(task.id), 0) : null,
  };
}
