import { useState } from 'react';
import { failure } from '../bridge/client';
import { ErrorText } from '../ui/controls';

export function NewWorkspaceFolder({
  parent,
  busy,
  onCreate,
  onCancel,
}: {
  parent: string;
  busy: boolean;
  onCreate: (name: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');
  return (
    <form
      className="new-workspace-folder"
      aria-label="New folder"
      onSubmit={(event) => {
        event.preventDefault();
        if (busy) return;
        if (!name.trim() || name === '.' || name === '..' || /[/\\\x00-\x1f\x7f-\x9f]/u.test(name)) {
          setError('Enter a single folder name without slashes or control characters.');
          return;
        }
        setError('');
        void onCreate(name).catch((error) => setError(failure(error).message));
      }}
    >
      <label className="field">
        <span>Folder name</span>
        <input
          autoFocus
          required
          value={name}
          disabled={busy}
          onChange={(event) => {
            setName(event.target.value);
            setError('');
          }}
        />
      </label>
      <div className="folder-parent muted" title={parent}>
        Create in {parent}
      </div>
      <ErrorText message={error} />
      <div className="actions">
        <button type="button" disabled={busy} onClick={onCancel}>
          Cancel new folder
        </button>
        <button className="primary" disabled={busy || !name.trim()}>
          {busy ? 'Creating…' : 'Create folder'}
        </button>
      </div>
    </form>
  );
}
