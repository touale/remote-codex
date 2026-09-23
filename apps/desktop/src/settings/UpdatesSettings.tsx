import { useEffect, useRef, useState } from 'react';
import { ArrowUpRight, LoaderCircle, RefreshCw } from 'lucide-react';
import { call, failure } from '../bridge/client';
import { installUpdate, type UpdateMode, type UpdateProgress, type UpdateSnapshot } from '../bridge/updates';
import { ErrorText } from '../ui/controls';
import { SelectField } from '../ui/SelectField';
import type { Ask } from '../ui/useDialog';

export function UpdatesSettings({ ask, onBusy }: { ask: Ask; onBusy: (busy: boolean) => void }) {
  const [snapshot, setSnapshot] = useState<UpdateSnapshot | null>(null);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [busy, setBusy] = useState<'checking' | 'installing' | 'restarting' | 'configuring' | 'confirming' | null>(
    null,
  );
  const [error, setError] = useState('');
  const revision = useRef(0);
  const running = useRef(false);
  const accept = async (operation: Promise<UpdateSnapshot>) => {
    const value = await operation;
    setSnapshot(value);
    return value;
  };
  const restart = async () => {
    setBusy('restarting');
    const result = await call('restart_app', { force: false });
    if (result.status === 'confirmation_required') {
      const tasks = [
        [result.sessions, 'active session'],
        [result.terminals, 'open terminal'],
        [result.transfers, 'file transfer'],
      ] as const;
      const summary = new Intl.ListFormat('en', { type: 'conjunction' }).format(
        tasks.filter(([count]) => count > 0).map(([count, label]) => `${count} ${label}${count === 1 ? '' : 's'}`),
      );
      setBusy('confirming');
      if (
        await ask({
          title: 'Tasks are still running',
          message: `There are ${summary} across your open windows. Restarting closes app sessions and terminals and pauses file transfers.`,
          choices: ['Restart anyway'],
        })
      ) {
        setBusy('restarting');
        await call('restart_app', { force: true });
      }
    }
  };
  const install = async () => {
    setBusy('installing');
    const value = await accept(installUpdate(setProgress));
    setProgress(null);
    if (!value.restart_required) return;
    setBusy('confirming');
    if (
      await ask({
        title: 'Restart to update?',
        message: `Remote Codex v${value.installed_version} is installed. Restart the app to use it.`,
        choices: ['Restart now'],
        cancelLabel: 'Later',
      })
    )
      await restart();
  };
  const check = async () => {
    const value = await accept(call('update_check', {}));
    if (!value.available || !value.latest) return;
    setBusy('confirming');
    const answer = await ask({
      title: 'Update available',
      message: value.can_install
        ? `Remote Codex v${value.latest.version} is available. Update now?`
        : `Remote Codex v${value.latest.version} is available. Download a release to update this development build.`,
      choices: [value.can_install ? 'Update now' : 'Open Releases'],
      cancelLabel: 'Later',
    });
    if (!answer) return;
    if (value.can_install) await install();
    else await call('external_link', { url: value.latest.url });
  };
  const run = async (activity: NonNullable<typeof busy>, operation: () => Promise<unknown>) => {
    if (running.current) return;
    running.current = true;
    ++revision.current;
    setBusy(activity);
    onBusy(true);
    setProgress(null);
    setError('');
    try {
      await operation();
    } catch (e) {
      setError(failure(e).message);
    } finally {
      running.current = false;
      setBusy(null);
      onBusy(false);
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
  const working = Boolean(busy) || snapshot.busy;
  return (
    <div className="general-settings">
      <section className="settings-group">
        <div className="setting-row">
          <div>
            <h3>Remote Codex</h3>
            <small>App version {snapshot.current_version}</small>
          </div>
          <button disabled={working} onClick={() => void run('checking', check)}>
            <RefreshCw size={14} className={busy === 'checking' ? 'spinning' : ''} />
            {busy === 'checking' ? 'Checking…' : 'Check for Updates'}
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
            onValueChange={(mode) =>
              void run('configuring', () => accept(call('update_configure', { mode: mode as UpdateMode })))
            }
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
              onClick={() => void run('installing', install)}
            >
              {busy === 'installing' ? <LoaderCircle size={14} className="spinning" /> : null}
              {busy === 'installing' ? 'Updating…' : 'Update App'}
            </button>
          )}
          {snapshot.latest && !snapshot.available && !snapshot.restart_required && (
            <small className="muted">The app is up to date.</small>
          )}
          {snapshot.restart_required && (
            <>
              <button className="primary" disabled={working} onClick={() => void run('restarting', restart)}>
                Restart now
              </button>
              <small className="muted">
                Version {snapshot.installed_version} is installed. The next launch uses the new version.
              </small>
            </>
          )}
        </div>
        <ErrorText message={error} />
      </section>
    </div>
  );
}
