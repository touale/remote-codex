import { expect, it } from 'vitest';
import type { RateLimit, TokenUsage } from '../bridge/session';
import { contextPercent, quotaWindow, remaining } from './format';
it('uses the native primary bucket and actual window durations, keeping context separate from totals', () => {
  const five = { used_percent: 22, window_minutes: 300, resets_at: 1234 };
  const week = { used_percent: 66, window_minutes: 10080, resets_at: 6789 };
  const limits: RateLimit[] = [
    { id: 'other', name: 'Other', is_default: false, primary: { ...five, used_percent: 90 }, secondary: null },
    { id: 'codex', name: 'Codex', is_default: true, primary: week, secondary: five },
  ];
  expect(remaining(quotaWindow(limits, 300)!)).toBe(78);
  expect(quotaWindow(limits, 10080)?.resets_at).toBe(6789);
  expect(quotaWindow([limits[0]], 300)).toBeNull();
  expect(quotaWindow([{ ...limits[1], primary: null, secondary: { ...five, window_minutes: null } }], 300)).toBeNull();
  expect(contextPercent({ last_tokens: 1000, total_tokens: 900000, context_window: 10000 } as TokenUsage)).toBe(10);
  expect(contextPercent(null)).toBeNull();
  expect(remaining({ ...five, used_percent: 120 })).toBe(0);
});
