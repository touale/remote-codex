import { ArrowUp } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { call, failure } from '../bridge/client';
import type { DirectoryPage, Entry } from '../bridge/types';
import { ErrorText, Modal } from '../ui/controls';
import { FolderIcon } from '../ui/FolderIcon';
import { remotePath, within } from './context';
import { FileTreeSkeleton } from './FileLoading';

export function MoveDialog({
  context,
  root,
  entry,
  onMove,
  onClose,
}: {
  context: string;
  root: string;
  entry: Entry;
  onMove: (entry: Entry, parent: string) => Promise<void>;
  onClose: () => void;
}) {
  const [path, setPath] = useState(entry.path.split('/').slice(0, -1).join('/'));
  const [page, setPage] = useState<DirectoryPage | null>(null);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [moving, setMoving] = useState(false);
  const pending = useRef(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setPage(null);
    setError('');
    void call('file_list', { context, path })
      .then(
        (page) => {
          if (!cancelled) setPage(page);
        },
        (error) => {
          if (!cancelled) setError(failure(error).message);
        },
      )
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [context, path, attempt]);
  const same = path === entry.path.split('/').slice(0, -1).join('/');
  const submit = async () => {
    if (pending.current || same || !page) return;
    pending.current = true;
    setMoving(true);
    setError('');
    try {
      await onMove(entry, path);
      onClose();
    } catch (error) {
      setError(failure(error).message);
    } finally {
      pending.current = false;
      setMoving(false);
    }
  };
  return (
    <Modal
      title={`Move ${entry.name}`}
      description="Choose a destination folder."
      onClose={() => {
        if (!pending.current) onClose();
      }}
    >
      <div className="workspace-path" title={remotePath(root, path)}>
        {remotePath(root, path)}
      </div>
      <div className="directory-picker" aria-label="Destination folders" aria-busy={loading}>
        <button
          className="picker-row"
          disabled={!path || moving}
          onClick={() => setPath(path.split('/').slice(0, -1).join('/'))}
        >
          <ArrowUp size={14} />
          Parent folder
        </button>
        {loading ? (
          <FileTreeSkeleton />
        ) : (
          page?.entries
            .filter((e) => e.directory && !e.symlink)
            .map((folder) => (
              <button
                className="picker-row"
                key={folder.path}
                disabled={moving || (entry.directory && within(entry.path, folder.path))}
                onClick={() => setPath(folder.path)}
              >
                <FolderIcon expanded={false} />
                {folder.name}
              </button>
            ))
        )}
        {page?.truncated && <small className="muted">Directory listing is limited.</small>}
      </div>
      <ErrorText message={error} />
      {error && !page && <button onClick={() => setAttempt((n) => n + 1)}>Retry</button>}
      <div className="actions">
        <button disabled={moving} onClick={onClose}>
          Cancel
        </button>
        <button className="primary" disabled={moving || loading || !page || same} onClick={() => void submit()}>
          {moving ? 'Moving…' : 'Move here'}
        </button>
      </div>
    </Modal>
  );
}
