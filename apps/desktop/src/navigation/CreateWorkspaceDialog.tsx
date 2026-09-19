import { useRef, useState } from 'react';
import { failure } from '../bridge/client';
import type { WorkspaceTarget } from '../bridge/files';
import { ErrorText, Modal } from '../ui/controls';
import { NewWorkspaceFolder } from './NewWorkspaceFolder';
import { useDirectoryBrowser } from './useDirectoryBrowser';

export function CreateWorkspaceDialog({
  parent,
  onOpen,
  onClose,
}: {
  parent: WorkspaceTarget;
  onOpen: (server: string, path: string) => Promise<unknown>;
  onClose: () => void;
}) {
  const [created, setCreated] = useState<string | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const browser = useDirectoryBrowser(parent.server, (error) => setError(failure(error).message));
  const run = async (action: () => Promise<void>) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      await action();
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  const open = async (path: string) => {
    setError('');
    try {
      await onOpen(parent.server, path);
      onClose();
    } catch (error) {
      setError(`Folder created, but the workspace could not be opened. ${failure(error).message}`);
    }
  };
  return (
    <Modal
      title={`New folder · ${parent.server}`}
      onClose={() => {
        if (!pending.current) onClose();
      }}
    >
      {created === null ? (
        <NewWorkspaceFolder
          parent={parent.path}
          busy={busy}
          onCancel={onClose}
          onCreate={(name) =>
            run(async () => {
              const path = await browser.create(parent.path, name);
              setCreated(path);
              await open(path);
            })
          }
        />
      ) : (
        <>
          <p className="folder-parent" title={created}>
            {created}
          </p>
          <div className="actions">
            <button disabled={busy} onClick={onClose}>
              Close
            </button>
            <button className="primary" disabled={busy} onClick={() => void run(() => open(created))}>
              {busy ? 'Opening…' : 'Retry opening'}
            </button>
          </div>
        </>
      )}
      <ErrorText message={error} />
    </Modal>
  );
}
