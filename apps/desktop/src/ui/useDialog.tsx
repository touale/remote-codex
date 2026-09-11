import { useRef, useState } from 'react';
import { Modal } from './controls';
interface Prompt {
  title: string;
  message?: string;
  input?: { label: string; value?: string };
  choices: string[];
}
export type Ask = (prompt: Prompt) => Promise<string | null>;
export function useDialog() {
  const [prompt, setPrompt] = useState<Prompt | null>(null);
  const [value, setValue] = useState('');
  const resolve = useRef<((answer: string | null) => void) | null>(null);
  const ask: Ask = (prompt) =>
    new Promise<string | null>((done) => {
      resolve.current?.(null);
      resolve.current = done;
      setValue(prompt.input?.value ?? '');
      setPrompt(prompt);
    });
  const finish = (answer: string | null) => {
    resolve.current?.(answer);
    resolve.current = null;
    setPrompt(null);
  };
  const dialog = prompt && (
    <Modal title={prompt.title} description={prompt.message} onClose={() => finish(null)}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          finish(prompt.input ? value : prompt.choices[0]);
        }}
      >
        {prompt.input && (
          <label className="field">
            <span>{prompt.input.label}</span>
            <input autoFocus required value={value} onChange={(event) => setValue(event.target.value)} />
          </label>
        )}
        <div className="actions">
          <button type="button" onClick={() => finish(null)}>
            Cancel
          </button>
          {prompt.choices.map((choice, index) => (
            <button
              type="button"
              className={index === 0 ? 'primary' : ''}
              key={choice}
              onClick={() => finish(prompt.input ? value : choice)}
            >
              {choice}
            </button>
          ))}
        </div>
      </form>
    </Modal>
  );
  return { ask, dialog };
}
