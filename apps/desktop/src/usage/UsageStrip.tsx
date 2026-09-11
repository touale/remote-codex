import type { ChatState, ChatUpdate } from '../chat/state';
import { fullTime } from '../chat/time';
import { useAppPreferences } from '../settings/preferences';
import { compactTokens, contextPercent, quotaWindow, remaining } from './format';
import { useNativeUsage } from './native';
import { useSessionUsage } from './session';

export function UsageStrip({
  chat,
  update,
  onDetails,
}: {
  chat?: ChatState;
  update?: ChatUpdate;
  onDetails: () => void;
}) {
  const prefs = useAppPreferences();
  const native = useNativeUsage(false);
  useSessionUsage(chat, update, prefs.five_hour_limit || prefs.weekly_limit);
  const limits = chat ? chat.status.limits : (native.value?.limits ?? []);
  const error = chat ? chat.status.limits_error : native.error;
  const at = chat ? chat.status.limits_updated_at : native.updatedAt;
  const stale = Boolean(error || chat?.closed || (at && Date.now() / 1000 - at > 120));
  const usage = chat?.status.usage ?? null;
  const context = contextPercent(usage);
  const quota = (label: string, minutes: number) => {
    const window = quotaWindow(limits, minutes);
    return {
      label: `${label} ${window ? `${remaining(window)}% left` : '—'}${window && stale ? ' · Cached' : ''}`,
      title: window
        ? `${window.resets_at == null ? 'Reset time unavailable.' : `Resets ${fullTime(window.resets_at)}.`}${at ? ` Updated ${fullTime(at)}.` : ''}`
        : 'Usage is unavailable. Open details for account information.',
    };
  };
  const items = [
    ...(prefs.five_hour_limit ? [quota('5h', 300)] : []),
    ...(prefs.weekly_limit ? [quota('Weekly', 10080)] : []),
    ...(prefs.context_usage
      ? [
          {
            label: `Context ${context == null ? '—' : `${context}%`}`,
            title: usage
              ? `${usage.last_tokens.toLocaleString()} / ${usage.context_window?.toLocaleString() ?? 'unknown'} tokens. Based on the latest native usage report.`
              : 'Context usage is available after Codex reports it.',
          },
        ]
      : []),
    ...(prefs.session_tokens
      ? [
          {
            label: `Tokens ${usage ? compactTokens(usage.total_tokens) : '—'}`,
            title: usage
              ? `${usage.total_tokens.toLocaleString()} total session tokens.`
              : 'Total tokens are available after Codex reports them.',
          },
        ]
      : []),
  ];
  if (!items.length) return null;
  return (
    <div className="usage-strip" aria-label="Conversation usage">
      {items.map((item) => (
        <button key={item.label.split(' ')[0]} onClick={onDetails} title={item.title}>
          {item.label}
        </button>
      ))}
    </div>
  );
}
