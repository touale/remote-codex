import type { RateLimit, RateWindow } from '../bridge/session';
import { fullTime } from '../chat/time';
import { remaining, windowLabel } from './format';
export function Limits({
  limits,
  updatedAt,
  error,
  loading,
  unavailable = 'Usage limits are unavailable for this account or provider.',
}: {
  limits: RateLimit[];
  updatedAt: number | null;
  error: string | null;
  loading: boolean;
  unavailable?: string;
}) {
  const window = (value: RateWindow | null, fallback: string) =>
    value && (
      <div className="limit-window">
        <div>
          <span>{windowLabel(value, fallback)}</span>
          <strong>{remaining(value)}% left</strong>
        </div>
        <progress aria-label={`${windowLabel(value, fallback)} remaining`} max={100} value={remaining(value)} />
        {value.resets_at != null && <small>Resets {fullTime(value.resets_at)}</small>}
      </div>
    );
  return (
    <div className="usage-limits">
      {limits.map((limit, index) => (
        <div className="usage-limit" key={limit.id ?? `${limit.name}-${index}`}>
          {limits.length > 1 && <strong>{limit.name}</strong>}
          {window(limit.primary, 'Primary limit')}
          {window(limit.secondary, 'Secondary limit')}
          {!limit.primary && !limit.secondary && <p className="muted">No usage windows reported.</p>}
        </div>
      ))}
      {!limits.length && <p className="muted">{loading ? 'Checking usage limits…' : unavailable}</p>}
      {updatedAt != null && (
        <small>
          {error ? 'Last available' : 'Updated'} {fullTime(updatedAt)}
        </small>
      )}
      {error && <p className="usage-error">Could not refresh usage. {error}</p>}
    </div>
  );
}
