import { Folder } from 'lucide-react';
import { useState } from 'react';
import { failure } from '../bridge/client';
import type { Catalog, Server, Workspace } from '../bridge/types';
import { ErrorText, Modal } from '../ui/controls';
export function WorkspacePicker({
  catalog,
  server = null,
  onSelect,
  onClose,
  onAdd,
}: {
  catalog: Catalog;
  server?: Server | null;
  onSelect: (workspace: Workspace) => Promise<void>;
  onClose: () => void;
  onAdd: (server: Server | null) => void;
}) {
  const servers = server ? [server] : catalog.servers;
  const workspaces = server ? catalog.workspaces.filter((w) => w.server_id === server.id) : catalog.workspaces;
  const [adding, setAdding] = useState(!workspaces.length);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  return (
    <Modal
      title={server ? `Choose a workspace · ${server.name}` : adding ? 'Choose a server' : 'Choose a workspace'}
      description={adding ? 'Choose the server for your new workspace.' : 'Select where to work on the remote server.'}
      onClose={onClose}
    >
      <div className="directory-picker">
        {adding &&
          servers.map((server) => (
            <button className="picker-row" key={server.id} onClick={() => onAdd(server)}>
              <span>
                {server.name}{' '}
                <small className="muted">
                  ({server.endpoint.host}:{server.endpoint.port ?? 22})
                </small>
              </span>
            </button>
          ))}
        {!adding &&
          workspaces.map((w) => (
            <button
              className="picker-row"
              key={`${w.server_id}:${w.path}`}
              disabled={busy}
              onClick={() => {
                setBusy(true);
                void onSelect(w)
                  .then(onClose)
                  .catch((e) => setError(failure(e).message))
                  .finally(() => setBusy(false));
              }}
            >
              <Folder size={14} />
              <span>
                {w.server} · {w.path}
              </span>
            </button>
          ))}
      </div>
      <ErrorText message={error} />
      <div className="actions">
        <button
          disabled={busy}
          onClick={() => {
            if (server) onAdd(server);
            else if (adding || !servers.length) onAdd(null);
            else if (servers.length === 1) onAdd(servers[0]);
            else setAdding(true);
          }}
        >
          {adding && !server ? 'Add server…' : 'Add workspace…'}
        </button>
        <button onClick={onClose}>Cancel</button>
      </div>
    </Modal>
  );
}
