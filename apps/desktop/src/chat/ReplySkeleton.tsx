import styles from './ConversationLoading.module.css';
/** Shared with history loading, without obscuring the already submitted message. */
export function ReplySkeleton() {
  return (
    <div className={styles.reply} aria-hidden="true">
      {[86, 74, 48].map((width) => (
        <div key={width} className={styles.block} style={{ width: `${width}%` }} />
      ))}
    </div>
  );
}
