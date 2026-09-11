import { useState } from 'react';
import type { AuthPrompt } from '../bridge/types';
import { Modal } from '../ui/controls';
export function Authentication({
  request,
  onAnswer,
}: {
  request: { id: string; prompt: AuthPrompt };
  onAnswer: (id: string, value: string | null) => void;
}) {
  const [value, setValue] = useState('');
  const host = request.prompt.kind === 'host_key';
  return (
    <Modal
      title={host ? 'Verify SSH host' : request.prompt.kind === 'passphrase' ? 'Unlock SSH key' : 'SSH authentication'}
      description={
        host
          ? 'Compare this fingerprint with a trusted source before connecting.'
          : 'Authentication is required to connect to this server.'
      }
      onClose={() => onAnswer(request.id, null)}
    >
      <pre className="auth-prompt">{request.prompt.message}</pre>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onAnswer(request.id, host ? 'yes' : value);
        }}
      >
        {!host && (
          <label className="field">
            <span>{request.prompt.kind === 'passphrase' ? 'Key passphrase' : 'Password'}</span>
            <input
              autoFocus
              type="password"
              autoComplete="off"
              value={value}
              onChange={(event) => setValue(event.target.value)}
            />
          </label>
        )}
        <div className="actions">
          <button type="button" onClick={() => onAnswer(request.id, null)}>
            Cancel
          </button>
          <button className="primary">{host ? 'Trust and connect' : 'Connect'}</button>
        </div>
      </form>
    </Modal>
  );
}
