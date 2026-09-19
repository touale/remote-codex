import { expect, it } from 'vitest';
import type { Transfer } from '../bridge/files';
import { transferSummary } from './summary';

const task = (values: Partial<Transfer> = {}): Transfer => ({
  id: 'upload',
  name: 'fixture',
  server: 'dev',
  workspace: '/project',
  direction: 'upload',
  destination: '',
  status: 'running',
  active: true,
  owned: true,
  bytes: 25,
  total: 100,
  files: 1,
  completed: 0,
  skipped: 0,
  message: null,
  error_code: null,
  conflict: null,
  ...values,
});

it('weights current transfers by bytes and counts only running speeds', () => {
  const tasks = [task(), task({ id: 'download', direction: 'download', bytes: 150, total: 300, owned: false })];
  const speed = (id: string) => (id === 'upload' ? 10 : 20);
  expect(transferSummary(tasks, speed)).toMatchObject({ active: 2, direction: null, percent: 43, speed: 30 });
  tasks[0] = task({ status: 'completed', bytes: 100, active: false });
  expect(transferSummary(tasks, speed)).toMatchObject({ active: 1, direction: 'download', percent: 50, speed: 20 });
  tasks.push(
    task({ id: 'cancelled', status: 'cancelled', total: 10000 }),
    task({ status: 'paused', active: false, total: 0 }),
  );
  expect(transferSummary(tasks, speed)).toMatchObject({ percent: 50, speed: 20, waiting: 1 });
  tasks[1] = { ...tasks[1], status: 'reconnecting' };
  expect(transferSummary(tasks, speed)).toMatchObject({ label: 'Reconnecting…', percent: 50, speed: null });
});

it('uses indeterminate progress when any active size is unknown or still being prepared', () => {
  for (const unknown of [task({ total: 0 }), task({ status: 'preparing' }), task({ status: 'queued' })])
    expect(transferSummary([task(), unknown], () => 0).percent).toBeNull();
  expect(transferSummary([task({ bytes: 120 })], () => 0).percent).toBe(100);
});

it('shows waiting and attention states without active progress or stale speed', () => {
  expect(transferSummary([task({ status: 'paused', active: false })], () => 100)).toMatchObject({
    label: 'Paused',
    active: 0,
    percent: null,
    speed: null,
  });
  for (const issue of [
    task({ status: 'conflict' }),
    task({ status: 'paused', active: false, error_code: 'SSH_FAILED' }),
  ])
    expect(transferSummary([issue], () => 100)).toMatchObject({
      label: 'Needs attention',
      attention: 1,
      active: 0,
      percent: null,
    });
  expect(transferSummary([task({ status: 'queued', active: false })], () => 0).label).toBe('Queued');
  expect(transferSummary([task({ status: 'completed', active: false })], () => 0)).toMatchObject({
    label: 'Transfers',
    active: 0,
    percent: null,
  });
});
