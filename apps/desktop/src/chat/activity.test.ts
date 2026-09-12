import { expect, it } from 'vitest';
import { activityTime } from './activity';

it('uses compact complete units and handles missing or skewed timestamps', () => {
  const now = 2_000_000;
  for (const [age, label] of [
    [0, '0s'],
    [59, '59s'],
    [60, '1m'],
    [3599, '59m'],
    [3600, '1h'],
    [86399, '23h'],
    [86400, '1d'],
    [259200, '3d'],
  ] as const) {
    expect(activityTime(now - age, now)).toBe(label);
  }
  expect(activityTime(now + 100, now)).toBe('0s');
  for (const timestamp of [0, -1, NaN, Infinity, Number.MAX_SAFE_INTEGER]) {
    expect(activityTime(timestamp, now)).toBe('—');
  }
});
