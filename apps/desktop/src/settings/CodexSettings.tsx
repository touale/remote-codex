import { useEffect, useState } from 'react';
import { call, failure, openLink } from '../bridge/client';
import type { NativeStatus, Preferences } from '../bridge/types';
import { invalidateSessionDefaults } from '../chat/drafts/defaults';
import { ErrorText } from '../ui/controls';
import { Disclosure } from '../ui/Disclosure';
import { AccountUsagePanel } from '../usage/AccountUsagePanel';
import { acceptNativeIdentity, invalidateNativeUsage, refreshNativeUsage } from '../usage/native';
export function CodexSettings({
  preferences,
  onPreferences,
}: {
  preferences: Preferences;
  onPreferences: (value: Partial<Preferences>) => void;
}) {
  const [native, setNative] = useState<NativeStatus | null>(null);
  const [path, setPath] = useState(preferences.codex_program ?? '');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [login, setLogin] = useState<string | null>(null);
  const refresh = () => {
    invalidateSessionDefaults();
    return call('native_status')
      .then((status) => {
        acceptNativeIdentity(status);
        setNative(status);
        void refreshNativeUsage();
        if (status.account.logged_in && login) {
          invalidateNativeUsage();
          void refreshNativeUsage(true);
          setLogin(null);
        }
      })
      .catch((error) => setError(failure(error).message));
  };
  useEffect(() => {
    void refresh();
  }, []);
  useEffect(() => {
    if (login) {
      const timer = setInterval(() => void refresh(), 2000);
      return () => clearInterval(timer);
    }
  }, [login]);
  const signIn = async () => {
    setBusy(true);
    setError('');
    try {
      invalidateNativeUsage();
      const result = await call('native_login', { cancelId: null });
      if (!result) throw new Error('Codex did not start login.');
      setLogin(result.id);
      await openLink(result.url);
    } catch (error) {
      setError(failure(error).message);
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <div className="settings-section">
        <h3>Account</h3>
        {native && (
          <>
            <div className="account-status">
              <span className={`status-dot ${native.account.logged_in ? 'online' : ''}`} />
              <span>{native.account.logged_in ? (native.account.email ?? 'Signed in') : 'Not signed in'}</span>
              <small>{native.account.plan}</small>
            </div>
          </>
        )}
        <div className="actions start">
          <button disabled={busy} onClick={() => void signIn()}>
            Sign in with ChatGPT
          </button>
          <button onClick={() => void refresh()}>Refresh status</button>
        </div>
        {login && (
          <div className="login-wait">
            Complete sign-in in your browser.
            <button
              onClick={() => {
                void call('native_login', { cancelId: login }).finally(() => setLogin(null));
              }}
            >
              Cancel sign-in
            </button>
          </div>
        )}
      </div>
      <AccountUsagePanel />
      <section className="settings-group codex-installation">
        <h3>Installation</h3>
        <div className="key-value">
          <span>Local Codex</span>
          <span>{native?.version ?? 'Checking…'}</span>
        </div>
        {native && (
          <div className="program-path" title={native.program}>
            {native.program}
          </div>
        )}
        <p className="settings-caption">Install and update with Codex’s native tools.</p>
        <Disclosure title="Codex executable">
          <label className="field">
            <span>Absolute local path</span>
            <input
              value={path}
              onChange={(event) => setPath(event.target.value)}
              placeholder="/Users/you/.local/bin/codex"
            />
          </label>
          <button
            disabled={!path.startsWith('/') || busy}
            onClick={() => {
              setBusy(true);
              setError('');
              void call('native_select', { path })
                .then((status) => {
                  invalidateNativeUsage();
                  invalidateSessionDefaults();
                  acceptNativeIdentity(status);
                  void refreshNativeUsage(true);
                  setNative(status);
                  onPreferences({ codex_program: status.program });
                })
                .catch((error) => setError(failure(error).message))
                .finally(() => setBusy(false));
            }}
          >
            Use this installation
          </button>
        </Disclosure>
      </section>
      <ErrorText message={error} />
    </>
  );
}
