import { useEffect, useRef, useState } from 'react';
import { ArrowUpRight, LoaderCircle, RefreshCw } from 'lucide-react';
import { call, failure } from '../bridge/client';
import { installUpdate, type UpdateMode, type UpdateProgress, type UpdateSnapshot } from '../bridge/updates';
import { ErrorText } from '../ui/controls';
import { SelectField } from '../ui/SelectField';

export function UpdatesSettings() {
  const [snapshot, setSnapshot] = useState<UpdateSnapshot | null>(null);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const revision = useRef(0);
  const run = async (operation: () => Promise<UpdateSnapshot>) => {
    ++revision.current;
    setBusy(true);
    setProgress(null);
    setError('');
    try {
      setSnapshot(await operation());
    } catch (e) {
      setError(failure(e).message);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };
  useEffect(() => {
    if (busy) return;
    let active = true;
    let pending = false;
    const refresh = async () => {
      if (document.hidden || pending) return;
      pending = true;
      const version = revision.current;
      try {
        const value = await call('update_status', {});
        if (active && version === revision.current) setSnapshot(value);
      } catch (e) {
        if (active && version === revision.current) setError(failure(e).message);
      } finally {
        pending = false;
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [busy]);
  if (!snapshot)
    return (
      <section className="settings-group" aria-busy="true">
        <h3>Remote Codex</h3>
        <p className="muted">Loading update settings…</p>
        <ErrorText message={error} />
      </section>
    );
  const working = busy || snapshot.busy;
  return (
    <div className="general-settings">
      <section className="settings-group">
        <div className="setting-row">
          <div>
            <h3>Remote Codex</h3>
            <small>App version {snapshot.current_version}</small>
          </div>
          <button disabled={working} onClick={() => void run(() => call('update_check', {}))}>
            <RefreshCw size={14} className={working ? 'spinning' : ''} /> Check for Updates
          </button>
        </div>
        <div className="setting-row">
          <label htmlFor="update-mode">
            <strong>Updates</strong>
            <small>The app and CLI share this setting. Each updates itself.</small>
          </label>
          <SelectField
            id="update-mode"
            label="Update mode"
            value={snapshot.mode}
            disabled={working}
            options={[
              { value: 'notify', label: 'Check automatically' },
              { value: 'auto', label: 'Update automatically' },
              { value: 'manual', label: 'Check manually' },
            ]}
            onValueChange={(mode) => void run(() => call('update_configure', { mode: mode as UpdateMode }))}
          />
        </div>
        {snapshot.latest && (
          <div className="setting-row">
            <div>
              <strong>Latest release · {snapshot.latest.version}</strong>
              <small>
                {snapshot.last_checked ? `Checked ${new Date(snapshot.last_checked * 1000).toLocaleString()}` : ''}
              </small>
            </div>
            <button
              onClick={() =>
                void call('external_link', { url: snapshot.latest!.url }).catch((e) => setError(failure(e).message))
              }
            >
              Release notes <ArrowUpRight size={14} />
            </button>
          </div>
        )}
        {!snapshot.can_install && (
          <p className="settings-caption">Development build. Install a GitHub Release to enable updates.</p>
        )}
        {snapshot.busy && !busy && <p className="settings-caption">An update is running in the background.</p>}
        {progress && (
          <div className="update-progress" role="status">
            <span>
              {progress.phase} · {Math.round((progress.received / progress.total) * 100)}%
            </span>
            <progress value={progress.received} max={progress.total} />
            <small>
              {(progress.received / 1048576).toFixed(1)} / {(progress.total / 1048576).toFixed(1)} MiB
            </small>
          </div>
        )}
        <div className="update-actions">
          {snapshot.available && (
            <button
              className="primary"
              disabled={working || !snapshot.can_install}
              onClick={() => void run(() => installUpdate(setProgress))}
            >
              {working ? <LoaderCircle size={14} className="spinning" /> : null} Update App
            </button>
          )}
          {snapshot.latest && !snapshot.available && !snapshot.restart_required && (
            <small className="muted">The app is up to date.</small>
          )}
          {snapshot.restart_required && (
            <small className="muted">
              Version {snapshot.installed_version} is installed. The next launch uses the new version.
            </small>
          )}
        </div>
        <ErrorText message={error} />
      </section>
    </div>
  );
}
