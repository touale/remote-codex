import { useEffect, useState } from 'react';
import { call, failure, operationId } from '../bridge/client';
import type { ServerAuthentication as Access, AuthenticationChange } from '../bridge/commands';
import { ErrorText } from '../ui/controls';
import { Disclosure } from '../ui/Disclosure';
export function ServerAuthentication({ name }: { name: string }) {
  const [access, setAccess] = useState<Access | null>(null);
  const [identity, setIdentity] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const change = async (change: AuthenticationChange | null) => {
    setBusy(true);
    setError('');
    try {
      const result = await call('server_authentication', { operationId: operationId(), name, change });
      setAccess(result);
      setIdentity(result.identity ?? '');
      setPassword('');
    } catch (error) {
      setError(failure(error).message);
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    void change(null);
  }, [name]);
  return (
    <Disclosure title="SSH authentication">
      <label className="field">
        <span>
          SSH key <small>Leave empty to use SSH defaults</small>
        </span>
        <input value={identity} onChange={(e) => setIdentity(e.target.value)} />
      </label>
      <button
        type="button"
        disabled={busy || !access || identity === (access.identity ?? '')}
        onClick={() => void change({ action: 'identity', path: identity || null })}
      >
        Verify and save key
      </button>
      <label className="field">
        <span>{access?.password_saved ? 'Replace saved password' : 'Save a password'}</span>
        <input
          type="password"
          autoComplete="new-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
      </label>
      <div className="actions start">
        <button
          type="button"
          disabled={busy || !password}
          onClick={() => void change({ action: 'password', value: password })}
        >
          Verify and save password
        </button>
        {access?.password_saved && (
          <button type="button" disabled={busy} onClick={() => void change({ action: 'forget_password' })}>
            Forget saved password
          </button>
        )}
      </div>
      <p className="muted">Passwords stay in Keychain. Changes apply to new connections.</p>
      <ErrorText message={error} />
    </Disclosure>
  );
}
