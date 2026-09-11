import type { RateLimit, RateWindow, TokenUsage } from '../bridge/session';
export const compactTokens = (value: number) =>
  new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 }).format(value);
export const remaining = (window: RateWindow) => Math.round(Math.max(0, Math.min(100, 100 - window.used_percent)));
export const windowLabel = (window: RateWindow, fallback: string) =>
  window.window_minutes === 300
    ? '5-hour limit'
    : window.window_minutes === 10080
      ? 'Weekly limit'
      : window.window_minutes
        ? `${window.window_minutes < 60 ? `${window.window_minutes} min` : `${window.window_minutes / 60} hr`} window`
        : fallback;
export function quotaWindow(limits: RateLimit[], minutes: number) {
  const primary = limits.find((limit) => limit.is_default) ?? limits.find((limit) => limit.id === 'codex');
  return [primary?.primary, primary?.secondary].find((window) => window?.window_minutes === minutes) ?? null;
}
export function contextPercent(usage: TokenUsage | null) {
  return usage?.context_window && usage.context_window > 0
    ? Math.round(Math.max(0, Math.min(100, (usage.last_tokens / usage.context_window) * 100)))
    : null;
}
