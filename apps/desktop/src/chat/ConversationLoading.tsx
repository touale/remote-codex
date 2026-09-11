import { AlertCircle } from 'lucide-react';
import type { WorkspaceTarget } from '../bridge/files';
import styles from './ConversationLoading.module.css';
import { ReplySkeleton } from './ReplySkeleton';

export interface SessionOpening {
  id: string;
  target: WorkspaceTarget;
  title: string;
  phase: 'opening' | 'history' | 'failed';
  error: string | null;
}
export function ConversationLoading({
  opening,
  onRetry,
  onBack,
}: {
  opening: SessionOpening;
  onRetry: () => void;
  onBack: () => void;
}) {
  const failed = opening.phase === 'failed';
  return (
    <section
      className={`chat ${styles.root}`}
      aria-label="Conversation"
      aria-busy={!failed}
      data-loading-phase={opening.phase}
    >
      {failed ? (
        <div className={styles.failure} role="alert">
          <AlertCircle size={20} aria-hidden="true" />
          <h2>Couldn’t open this conversation</h2>
          <p>{opening.error}</p>
          <div className="actions">
            <button onClick={onBack}>Back</button>
            <button className="primary" onClick={onRetry}>
              Retry
            </button>
          </div>
        </div>
      ) : (
        <>
          <span className="sr-only" role="status">
            Loading conversation…
          </span>
          <div className={styles.messages} aria-hidden="true">
            <div className={`${styles.block} ${styles.user}`} />
            <ReplySkeleton />
            <ReplySkeleton />
          </div>
          <div className={styles.composer} aria-hidden="true">
            <div className={`${styles.block} ${styles.input}`} />
            <div className={styles.controls}>
              <div className={styles.block} style={{ width: 58 }} />
              <div className={styles.block} style={{ width: 92 }} />
              <div className={`${styles.block} ${styles.send}`} />
            </div>
          </div>
        </>
      )}
    </section>
  );
}
