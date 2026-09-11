import { ArrowUp, ChevronRight, Folder, FolderPlus } from 'lucide-react';
import { useState } from 'react';
import { failure } from '../bridge/client';
import type { DirectoryPage, Server } from '../bridge/types';
import { ErrorText, Modal } from '../ui/controls';
import { NewWorkspaceFolder } from './NewWorkspaceFolder';
import { useDirectoryBrowser } from './useDirectoryBrowser';

export function WorkspaceDialog({
  server,
  onClose,
  onOpen,
}: {
  server: Server;
  onClose: () => void;
  onOpen: (server: string, path: string) => Promise<unknown>;
}) {
  const [path, setPath] = useState('/');
  const [page, setPage] = useState<{ path: string; contents: DirectoryPage } | null>(null);
  const [error, setError] = useState('');
  const browser = useDirectoryBrowser(server.name, (error) => setError(failure(error).message));
  const [pending, setPending] = useState<'browse' | 'create' | 'open' | null>(null);
  const [creating, setCreating] = useState(false);
  const busy = pending !== null;
  const listed = page?.path === path ? page.contents : null;
  const browse = async (next: string) => {
    if (busy) return;
    setPending('browse');
    setError('');
    setCreating(false);
    setPath(next);
    setPage(null);
    try {
      const contents = await browser.list(next);
      setPage({ path: next, contents });
    } catch (error) {
      setError(failure(error).message);
    } finally {
      setPending(null);
    }
  };
  const create = async (name: string) => {
    if (busy || !listed) return;
    setPending('create');
    setError('');
    try {
      const created = await browser.create(path, name);
      setCreating(false);
      setPath(created);
      setPage(null);
      try {
        const contents = await browser.list(created);
        setPage({ path: created, contents });
      } catch (error) {
        setError(`Folder created, but it could not be listed. ${failure(error).message}`);
      }
    } finally {
      setPending(null);
    }
  };
  const directories = listed?.entries.filter((entry) => entry.directory);
  return (
    <Modal
      title={`Add workspace · ${server.name}`}
      description="Choose a directory on this server, or browse to create a new folder."
      onClose={() => {
        if (pending !== 'create' && pending !== 'open') onClose();
      }}
      className="workspace-dialog"
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void browse(path);
        }}
      >
        <label className="field">
          <span>Remote directory</span>
          <div className="path-input">
            <input
              autoFocus
              required
              disabled={busy || creating}
              value={path}
              onChange={(event) => {
                setPath(event.target.value);
                setPage(null);
                setError('');
              }}
              placeholder="/workspace/project"
            />
            <button disabled={busy || creating || !path.startsWith('/')}>
              {pending === 'browse' ? 'Browsing…' : 'Browse'}
            </button>
          </div>
        </label>
      </form>
      {listed && (
        <>
          <div className="directory-toolbar">
            <button
              disabled={busy || creating || /^\/+$/u.test(path)}
              onClick={() => void browse(path.replace(/\/+$/, '').split('/').slice(0, -1).join('/') || '/')}
            >
              <ArrowUp size={14} />
              Parent directory
            </button>
            <button
              disabled={busy || creating}
              onClick={() => {
                setCreating(true);
                setError('');
              }}
            >
              <FolderPlus size={14} />
              New folder
            </button>
          </div>
          {creating && (
            <NewWorkspaceFolder parent={path} busy={busy} onCreate={create} onCancel={() => setCreating(false)} />
          )}
          <div className="directory-picker" aria-label="Remote folders" aria-busy={busy}>
            {directories?.map((entry) => (
              <button
                className="picker-row"
                key={entry.path}
                disabled={busy || creating}
                onClick={() => void browse(`${path.replace(/\/+$/, '')}/${entry.name}`)}
              >
                <Folder size={14} />
                <span title={entry.name}>{entry.name}</span>
                <ChevronRight size={14} />
              </button>
            ))}
            {directories?.length === 0 && <div className="tree-hint">No folders in this directory.</div>}
            {listed.truncated && <small>More entries are available. Enter a path directly.</small>}
          </div>
        </>
      )}
      <ErrorText message={error} />
      <div className="actions">
        <button disabled={pending === 'create' || pending === 'open'} onClick={onClose}>
          Cancel
        </button>
        <button
          className="primary"
          disabled={busy || creating || !path.startsWith('/')}
          onClick={() => {
            setPending('open');
            setError('');
            void onOpen(server.name, path)
              .then(onClose)
              .catch((error) => setError(failure(error).message))
              .finally(() => setPending(null));
          }}
        >
          {pending === 'open' ? 'Opening…' : 'Open workspace'}
        </button>
      </div>
    </Modal>
  );
}
