import { ChevronUp, X } from 'lucide-react';
import { Popover } from 'radix-ui';
import { useCallback, useState, useSyncExternalStore } from 'react';
import { call } from '../bridge/client';
import type { Progress } from '../bridge/types';
import type { DraftStore } from '../chat/drafts/store';
import type { ChatSummary } from '../chat/store';
import { IconButton } from '../ui/controls';
import { contextStatus, operationStatus, type Status } from './status';
import styles from './StatusBar.module.css';

export function StatusBar({
  progress,
  transfers,
  drafts,
  draftKey,
  busy,
  openingPhase,
  ready,
  workspace,
  serverFiles,
  chat,
  error,
  authenticating,
  report,
}: {
  progress: Record<string, Progress>;
  transfers: React.ReactNode;
  drafts: DraftStore;
  draftKey: string | null;
  busy: boolean;
  openingPhase?: 'opening' | 'history' | 'failed';
  ready: boolean;
  workspace: boolean;
  serverFiles?: boolean;
  chat?: ChatSummary;
  error: string;
  authenticating: boolean;
  report: (error: unknown) => void;
}) {
  const phase = useSyncExternalStore(
    useCallback((listener) => (draftKey ? drafts.subscribe(draftKey, listener) : () => {}), [drafts, draftKey]),
    useCallback(() => {
      const draft = draftKey ? drafts.get(draftKey) : null;
      return draft?.preparing ? draft.phase : draft?.error ? 'failed' : null;
    }, [drafts, draftKey]),
  );
  // Updating an existing operation preserves its insertion order.
  const operations = Object.entries(progress).reverse();
  const current = operations[0];
  const transfer = current ? operationStatus(current[1]) : null;
  const status: Status = authenticating
    ? { label: 'Waiting for SSH authentication…', tone: 'busy' }
    : transfer
      ? { label: transfer.label, tone: 'busy' }
      : openingPhase
        ? {
            label: {
              opening: 'Opening conversation…',
              history: 'Loading conversation history…',
              failed: 'Conversation could not be loaded',
            }[openingPhase],
            tone: openingPhase === 'failed' ? 'attention' : 'busy',
          }
        : phase
          ? {
              label: {
                connecting: 'Preparing session…',
                settings: 'Applying session settings…',
                submitting: 'Sending first message…',
                failed: 'Message not sent',
              }[phase],
              tone: phase === 'failed' ? 'attention' : 'busy',
            }
          : contextStatus({
              ready,
              busy,
              workspace,
              serverFiles,
              error,
              closed: chat?.closed,
              environment: chat?.environment,
            });
  return (
    <footer className={styles.bar} aria-label="Workspace status" data-session-id={chat?.session.id}>
      <div className={styles.summary} role="status" aria-live="polite" aria-atomic="true" data-state={status.tone}>
        <span
          aria-hidden="true"
          className={status.tone === 'busy' ? 'pulse' : `status-dot ${status.tone === 'online' ? 'online' : ''}`}
        />
        <span className={styles.label} title={status.label}>
          {status.label}
        </span>
      </div>
      {transfer?.percent != null && !authenticating && (
        <progress aria-label="Transfer progress" max={100} value={transfer.percent} />
      )}
      {current && <CancelOperation key={current[0]} id={current[0]} report={report} />}
      {operations.length > 1 && (
        <Popover.Root>
          <Popover.Trigger asChild>
            <button className={styles.operations} aria-label={`Show ${operations.length} active operations`}>
              {operations.length} operations <ChevronUp size={12} />
            </button>
          </Popover.Trigger>
          <Popover.Portal>
            <Popover.Content
              className={styles.popover}
              side="top"
              align="end"
              sideOffset={8}
              collisionPadding={12}
              aria-label="Active operations"
            >
              <h2>Active operations</h2>
              <ul>
                {operations.map(([id, event]) => {
                  const item = operationStatus(event);
                  return (
                    <li key={id}>
                      <span className="pulse" aria-hidden="true" />
                      <span className={styles.label} title={item.label}>
                        {item.label}
                      </span>
                      {item.percent !== null && (
                        <progress aria-label="Transfer progress" max={100} value={item.percent} />
                      )}
                      <CancelOperation id={id} report={report} />
                    </li>
                  );
                })}
              </ul>
            </Popover.Content>
          </Popover.Portal>
        </Popover.Root>
      )}
      {transfers}
      {chat && (
        <button
          className={styles.session}
          title="Copy session ID"
          onClick={() => void navigator.clipboard.writeText(chat.session.id).catch(report)}
        >
          {chat.session.id.slice(0, 8)}
        </button>
      )}
    </footer>
  );
}

function CancelOperation({ id, report }: { id: string; report: (error: unknown) => void }) {
  const [cancelling, setCancelling] = useState(false);
  return (
    <IconButton
      className={styles.cancel}
      label={cancelling ? 'Cancelling operation…' : 'Cancel operation'}
      disabled={cancelling}
      onClick={() => {
        setCancelling(true);
        void call('cancel_operation', { id }).catch((error) => {
          setCancelling(false);
          report(error);
        });
      }}
    >
      <X size={12} />
    </IconButton>
  );
}
