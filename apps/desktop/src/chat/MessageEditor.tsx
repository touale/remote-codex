import { useLayoutEffect, useRef } from 'react';
import { failure } from '../bridge/client';
import { ErrorText } from '../ui/controls';
import type { ChatState, ChatUpdate, MessageEdit } from './state';

export function MessageEditor({
  edit,
  update,
  onRevert,
  onReload,
  onSend,
  enabled,
}: {
  edit: MessageEdit;
  update: ChatUpdate;
  onRevert: (turn: string) => Promise<void>;
  onReload: () => Promise<void>;
  onSend: (text: string, clientId: string) => Promise<void>;
  enabled: boolean;
}) {
  const input = useRef<HTMLTextAreaElement>(null);
  const pending = useRef(false);
  const busy = !!edit.busy;
  const setBusy = (busy: boolean) => change({ busy });
  const setError = (error: string) => change({ error });
  useLayoutEffect(() => input.current?.focus(), []);
  const change = (patch: Partial<MessageEdit>) =>
    update((current) =>
      current.edit?.clientId === edit.clientId ? { ...current, edit: { ...current.edit, ...patch } } : current,
    );
  const send = async () => {
    if (!enabled || pending.current || edit.busy || edit.uncertain || !edit.text.trim()) return;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      if (!edit.reverted) await onRevert(edit.turn);
      await onSend(edit.text.trim(), edit.clientId);
      update({ edit: undefined });
    } catch (e) {
      setError(failure(e).message);
      if (failure(e).outcome_unknown) change({ uncertain: true });
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  return (
    <form
      className="message-editor"
      onSubmit={(e) => {
        e.preventDefault();
        void send();
      }}
    >
      <textarea
        ref={input}
        aria-label="Edit message"
        rows={4}
        value={edit.text}
        disabled={busy}
        onChange={(e) => change({ text: e.target.value })}
      />
      <small>Resending replaces the conversation from this message onward. Remote file changes are kept.</small>
      <ErrorText message={edit.error} />
      <div className="actions">
        <button type="button" disabled={busy} onClick={() => update({ edit: undefined })}>
          Cancel
        </button>
        {edit.uncertain ? (
          <button
            type="button"
            onClick={() => {
              if (pending.current) return;
              pending.current = true;
              setBusy(true);
              void onReload()
                .catch((e) => setError(failure(e).message))
                .finally(() => {
                  pending.current = false;
                  setBusy(false);
                });
            }}
            disabled={busy}
          >
            Reload history
          </button>
        ) : (
          <button className="primary" disabled={busy || !enabled || !edit.text.trim()}>
            {busy ? 'Sending…' : edit.reverted ? 'Send message' : 'Save & resend'}
          </button>
        )}
      </div>
    </form>
  );
}

export function startEdit(chat: ChatState, id: string): ChatState {
  const message = chat.messages.find((m) => m.id === id);
  if (!message?.turn) return chat;
  const turns = [...new Set(chat.messages.map((m) => m.turn).filter((t): t is string => !!t))];
  return {
    ...chat,
    edit: {
      id,
      clientId: crypto.randomUUID(),
      turn: message.turn,
      text: message.text,
      removedTurns: turns.slice(turns.indexOf(message.turn)),
    },
  };
}
