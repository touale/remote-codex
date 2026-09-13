import { useEffect, useState } from 'react';
import { call, failure, operationId } from '../bridge/client';
import type { ConfigReport, Server } from '../bridge/types';
import { ErrorText, Modal } from '../ui/controls';
import { ServerAuthentication } from './ServerAuthentication';
import { serverSettingsChanges } from './serverSettings';
export function ServerDialog({
  server,
  onClose,
  onSaved,
}: {
  server: Server | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState(server?.name ?? '');
  const [address, setAddress] = useState(
    server ? `${server.endpoint.user ? `${server.endpoint.user}@` : ''}${server.endpoint.host}` : '',
  );
  const [port, setPort] = useState(String(server?.endpoint.port ?? 22));
  const [identity, setIdentity] = useState('');
  const [password, setPassword] = useState('');
  const [proxy, setProxy] = useState('');
  const [useProxy, setUseProxy] = useState(false);
  const [background, setBackground] = useState(true);
  const [retryAttempts, setRetryAttempts] = useState('10');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [config, setConfig] = useState<ConfigReport | null>(null);
  useEffect(() => {
    if (server)
      void call('server_config', { name: server.name, updates: null, revision: null })
        .then((config) => {
          setConfig(config);
          const value = (key: string) => config.items.find((item) => item.key === key)?.value;
          setUseProxy(value('proxy.mode') === 'custom');
          setProxy(String(value('env.https_proxy') ?? ''));
          setBackground(value('background') !== false);
          setRetryAttempts(String(value('reconnect.max_attempts') ?? 10));
        })
        .catch((error) => setError(failure(error).message));
  }, [server]);
  const submit = async () => {
    if (busy || (server && !config)) return;
    setBusy(true);
    setError('');
    const settings = serverSettingsChanges({ useProxy, proxy, background, retryAttempts }, config);
    try {
      if (server)
        await call('server_config', { name: server.name, updates: settings, revision: config?.saved_revision ?? null });
      else
        await call('server_save', {
          operationId: operationId(),
          input: {
            name,
            address,
            port: Number(port),
            identity: identity || null,
            password: password || null,
            settings,
          },
        });
      setPassword('');
      onSaved();
    } catch (error) {
      setError(failure(error).message);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={server ? `Server settings · ${server.name}` : 'Add server'}
      description="Connect a remote execution environment over SSH."
      onClose={onClose}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <label className="field">
          <span>Name</span>
          <input
            autoFocus
            required
            value={name}
            disabled={Boolean(server)}
            placeholder="development"
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <div className="form-grid">
          <label className="field">
            <span>SSH address</span>
            <input
              required
              disabled={Boolean(server)}
              value={address}
              placeholder="user@hostname"
              onChange={(event) => setAddress(event.target.value)}
            />
          </label>
          <label className="field port">
            <span>Port</span>
            <input
              type="number"
              min={1}
              max={65535}
              required
              disabled={Boolean(server)}
              value={port}
              onChange={(event) => setPort(event.target.value)}
            />
          </label>
        </div>
        {!server && (
          <>
            <label className="field">
              <span>
                SSH key <small>Optional · otherwise use SSH agent or password</small>
              </span>
              <input
                value={identity}
                placeholder="/Users/you/.ssh/id_ed25519"
                onChange={(event) => setIdentity(event.target.value)}
              />
            </label>
            <label className="field">
              <span>
                Password <small>Optional · saved securely in Keychain</small>
              </span>
              <input
                type="password"
                autoComplete="new-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </label>
          </>
        )}
        <label className="check-field">
          <input type="checkbox" checked={useProxy} onChange={(event) => setUseProxy(event.target.checked)} />
          Use a proxy for remote commands
        </label>
        {useProxy && (
          <label className="field">
            <span>Remote proxy URL</span>
            <input
              type="url"
              required
              value={proxy}
              placeholder="http://127.0.0.1:7890"
              onChange={(event) => setProxy(event.target.value)}
            />
          </label>
        )}
        {server && (
          <label className="check-field">
            <input type="checkbox" checked={background} onChange={(event) => setBackground(event.target.checked)} />
            Keep submitted commands running after disconnect
          </label>
        )}
        <label className="field">
          <span>
            Retry attempts <small>Retry every 5 seconds. Changes apply to the next recovery.</small>
          </span>
          <input
            type="number"
            min={1}
            max={65535}
            step={1}
            required
            value={retryAttempts}
            onChange={(event) => setRetryAttempts(event.target.value)}
          />
        </label>
        {server && <ServerAuthentication name={server.name} />}
        <ErrorText message={error} />
        <div className="actions">
          <button type="button" disabled={busy} onClick={onClose}>
            Cancel
          </button>
          <button className="primary" disabled={busy || Boolean(server && !config)}>
            {busy ? (server ? 'Saving…' : 'Connecting…') : server ? 'Save settings' : 'Add server'}
          </button>
        </div>
      </form>
    </Modal>
  );
}
