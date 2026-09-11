import { Plus, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { call, failure } from '../bridge/client';
import type { McpConfig, McpStatus } from '../bridge/types';
import { ErrorText, IconButton } from '../ui/controls';
export function McpSettings({
  workspace,
  session,
  onDirty,
}: {
  workspace: string;
  session: string | null;
  onDirty: (dirty: boolean) => void;
}) {
  const [config, setConfig] = useState<McpConfig>({ servers: [], revision: null });
  const [saved, setSaved] = useState('[]');
  const [statuses, setStatuses] = useState<McpStatus[]>([]);
  const [selected, setSelected] = useState(0);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState('');
  useEffect(() => {
    void call('project_mcp', { workspace, servers: null, revision: null })
      .then((value) => {
        setConfig(value);
        setSaved(JSON.stringify(value.servers));
      })
      .catch((error) => setError(failure(error).message))
      .finally(() => setBusy(false));
    if (session)
      void call('session_mcp', { id: session })
        .then(setStatuses)
        .catch((error) => setError(failure(error).message));
  }, [workspace, session]);
  useEffect(() => onDirty(JSON.stringify(config.servers) !== saved), [config.servers, saved, onDirty]);
  const server = config.servers[selected];
  const update = (key: 'name' | 'definition', value: string) =>
    setConfig((previous) => ({
      ...previous,
      servers: previous.servers.map((s, index) => (index === selected ? { ...s, [key]: value } : s)),
    }));
  return (
    <>
      <p className="muted">
        Remote project stdio servers. Changes apply when reopening a session and reviewing its trust request.
      </p>
      <fieldset disabled={busy} className="settings-fields">
        <div className="mcp-manager">
          <div className="mcp-list">
            {config.servers.map((server, index) => (
              <button key={index} className={selected === index ? 'selected' : ''} onClick={() => setSelected(index)}>
                {server.name || 'Unnamed server'}
              </button>
            ))}
            <button
              onClick={() => {
                setSelected(config.servers.length);
                setConfig({
                  ...config,
                  servers: [...config.servers, { name: '', definition: 'command = ""\nargs = []\nenabled = true\n' }],
                });
              }}
            >
              <Plus size={14} /> Add server
            </button>
          </div>
          <div className="mcp-definition">
            {server ? (
              <>
                <div className="form-grid">
                  <label className="field">
                    <span>Name</span>
                    <input value={server.name} onChange={(event) => update('name', event.target.value)} />
                  </label>
                  <IconButton
                    label="Remove MCP server"
                    onClick={() => {
                      setConfig({ ...config, servers: config.servers.filter((_, index) => index !== selected) });
                      setSelected(0);
                    }}
                  >
                    <Trash2 size={15} />
                  </IconButton>
                </div>
                <label className="field">
                  <span>Stdio configuration · TOML</span>
                  <textarea
                    className="code-input"
                    rows={12}
                    spellCheck={false}
                    value={server.definition}
                    onChange={(event) => update('definition', event.target.value)}
                  />
                </label>
                {statuses.find((s) => s.name === server.name) && (
                  <small>{statuses.find((s) => s.name === server.name)?.status}</small>
                )}
              </>
            ) : (
              <p className="muted">No remote MCP servers configured.</p>
            )}
          </div>
        </div>
      </fieldset>
      <ErrorText message={error} />
      <div className="actions">
        <button
          className="primary"
          disabled={busy || JSON.stringify(config.servers) === saved}
          onClick={() => {
            setBusy(true);
            setError('');
            void call('project_mcp', { workspace, servers: config.servers, revision: config.revision })
              .then((value) => {
                setConfig(value);
                setSaved(JSON.stringify(value.servers));
              })
              .catch((error) => setError(failure(error).message))
              .finally(() => setBusy(false));
          }}
        >
          {busy ? 'Working…' : 'Save configuration'}
        </button>
      </div>
    </>
  );
}
