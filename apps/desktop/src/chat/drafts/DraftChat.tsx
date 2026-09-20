import { useCallback, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import type { SessionAction } from '../../bridge/session';
import type { SessionDefaults } from '../../bridge/types';
import { Empty, Modal } from '../../ui/controls';
import { AccountUsagePanel } from '../../usage/AccountUsagePanel';
import { UsageStrip } from '../../usage/UsageStrip';
import { Composer } from '../composer/Composer';
import type { ComposerState } from '../composer/model';
import { PendingMessage } from './PendingMessage';
import { useSessionDefaults } from './defaults';
import type { DraftStore } from './store';
import type { DraftSubmission } from './submit';

export function DraftChat({
  draftKey,
  store,
  submit,
}: {
  draftKey: string;
  store: DraftStore;
  submit: (key: string, action: DraftSubmission) => Promise<void>;
}) {
  const subscribe = useCallback((listener: () => void) => store.subscribe(draftKey, listener), [draftKey, store]);
  const snapshot = useCallback(() => store.get(draftKey), [draftKey, store]);
  const draft = useSyncExternalStore(subscribe, snapshot);
  const [status, setStatus] = useState(false);
  const preview = useSessionDefaults(draft?.target.server ?? '', Boolean(draft && !draft.pending));
  const lastPreview = useRef<SessionDefaults | null>(null);
  if (preview.value) lastPreview.current = preview.value;
  const defaults = preview.value ?? (draft?.pending ? lastPreview.current : null);
  const scroll = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (draft?.pending && scroll.current) scroll.current.scrollTop = scroll.current.scrollHeight;
  }, [draft?.pending, draft?.preparing]);

  if (!draft) return null;
  const action = async (action: SessionAction) => {
    if (action.action === 'settings')
      store.update(draftKey, (value) => ({ ...value, settings: { ...value.settings, ...action.settings } }));
    else if (action.action === 'submit' || action.action === 'goal') await submit(draftKey, action);
  };
  const state: ComposerState = {
    server: draft.target.server,
    draft: draft.text,
    composerMode: draft.mode,
    goal: null,
    turn: null,
    ready: true,
    models: defaults?.models ?? [],
    settingsState: defaults ? 'ready' : preview.error ? 'error' : 'loading',
    settings: {
      mode: draft.settings.mode ?? 'agent',
      model: draft.settings.model ?? defaults?.settings.model ?? null,
      effort: draft.settings.effort ?? defaults?.settings.effort ?? null,
      full_access: draft.settings.permissions
        ? draft.settings.permissions === 'full_access'
        : (defaults?.settings.full_access ?? null),
      approval_policy: draft.settings.permissions
        ? draft.settings.permissions === 'full_access'
          ? 'never'
          : 'on-request'
        : (defaults?.settings.approval_policy ?? null),
      reviewer: draft.settings.reviewer ?? defaults?.settings.reviewer ?? null,
    },
  };
  return (
    <section className="chat" aria-label="New conversation">
      <div className="chat-scroll" ref={scroll}>
        <div className="messages">
          {draft.pending ? (
            <PendingMessage draft={draft} store={store} draftKey={draftKey} submit={submit} />
          ) : (
            <Empty title="What would you like to work on?" detail={`${draft.target.server} · ${draft.target.path}`} />
          )}
        </div>
      </div>
      <Composer
        focusRequest={draft.focus}
        errorMessage={draft.pending ? '' : draft.error}
        ownsSubmissionDraft
        chat={state}
        preparing={Boolean(draft.pending)}
        onAction={action}
        onStatus={() => setStatus(true)}
        setDraft={(text, expected) =>
          store.update(draftKey, (current) => ({
            ...current,
            text: expected === undefined || current.text === expected ? text : current.text,
          }))
        }
        setComposerMode={(mode) => store.update(draftKey, (current) => ({ ...current, mode }))}
        details={
          <>
            {preview.error && !draft.pending && (
              <div className="defaults-error" role="alert">
                <span>Couldn’t load session settings. {preview.error}</span>
                <button onClick={() => void preview.retry()}>Retry settings</button>
              </div>
            )}
            <UsageStrip onDetails={() => setStatus(true)} />
          </>
        }
      />
      {status && (
        <Modal
          title="New conversation"
          description={
            draft.opened
              ? 'Your session is ready. No message has been submitted yet.'
              : 'This conversation has not started yet.'
          }
          onClose={() => setStatus(false)}
        >
          <p>
            {draft.target.server} · {draft.target.path}
          </p>
          {draft.opened && <code>{draft.opened.id}</code>}
          <AccountUsagePanel />
          <p className="muted">Send a message to start Codex.</p>
        </Modal>
      )}
    </section>
  );
}
