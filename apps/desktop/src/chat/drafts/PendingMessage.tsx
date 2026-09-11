import { useState } from 'react';
import { Modal } from '../../ui/controls';
import { ReplySkeleton } from '../ReplySkeleton';
import { clockTime, fullTime } from '../time';
import type { Draft, DraftStore } from './store';
import type { DraftSubmission } from './submit';
export function PendingMessage({
  draft,
  store,
  draftKey,
  submit,
}: {
  draft: Draft;
  store: DraftStore;
  draftKey: string;
  submit: (key: string, action: DraftSubmission) => Promise<void>;
}) {
  const [edit, setEdit] = useState<string | null>(null);
  const pending = draft.pending;
  if (!pending) return null;
  return (
    <>
      <article className="message user" data-pending-submission="">
        <div className="message-body pending-message">{pending.text}</div>
        <time
          className="message-time"
          title={fullTime(pending.sentAt)}
          dateTime={new Date(pending.sentAt * 1000).toISOString()}
        >
          {clockTime(pending.sentAt)}
        </time>
      </article>
      {draft.preparing ? (
        <div aria-busy="true" aria-label="Preparing first message">
          <span className="sr-only" role="status">
            Preparing your session…
          </span>
          <ReplySkeleton />
        </div>
      ) : (
        draft.error && (
          <div className="submission-error" role="alert">
            <p>{draft.error}</p>
            <div className="actions start">
              <button onClick={() => void submit(draftKey, pending.action).catch(() => {})}>Retry</button>
              <button onClick={() => setEdit(pending.text)}>Edit</button>
              <button onClick={() => store.update(draftKey, (value) => ({ ...value, pending: null, error: null }))}>
                Discard
              </button>
            </div>
          </div>
        )
      )}
      {edit !== null && (
        <Modal
          title="Edit unsent message"
          description="Your next draft stays in the composer."
          onClose={() => setEdit(null)}
        >
          <label className="field">
            <span>Message</span>
            <textarea
              autoFocus
              aria-label="Unsent message"
              rows={6}
              value={edit}
              onChange={(event) => setEdit(event.target.value)}
            />
          </label>
          <div className="actions">
            <button onClick={() => setEdit(null)}>Cancel</button>
            <button
              className="primary"
              disabled={!edit.trim()}
              onClick={() => {
                const text = edit.trim();
                store.update(draftKey, (value) => {
                  if (!value.pending || value.preparing) return value;
                  const previous = value.pending;
                  const action: DraftSubmission =
                    previous.action.action === 'submit'
                      ? { action: 'submit', text }
                      : {
                          action: 'goal',
                          goal: {
                            action: 'set',
                            objective: text,
                            token_budget:
                              previous.action.goal.action === 'set' ? previous.action.goal.token_budget : null,
                          },
                        };
                  return { ...value, pending: { ...previous, text, action } };
                });
                setEdit(null);
              }}
            >
              Save message
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
