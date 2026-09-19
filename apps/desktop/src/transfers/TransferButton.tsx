import {
  ArrowDownToLine,
  ArrowDownUp,
  ArrowUpFromLine,
  CircleAlert,
  FolderOpen,
  Pause,
  Play,
  Trash2,
  X,
} from 'lucide-react';
import { Popover } from 'radix-ui';
import { useState } from 'react';
import { call } from '../bridge/client';
import type { Choice, SkippedTransfers, Transfer } from '../bridge/files';
import { IconButton } from '../ui/controls';
import { transferStore, useTransferList } from './store';
import { finished, transferSummary } from './summary';

import styles from './transfers.module.css';
import type { TransferActions } from './useTransfers';
export function TransferButton({ actions, report }: { actions: TransferActions; report: (error: unknown) => void }) {
  const tasks = useTransferList();
  const [clearing, setClearing] = useState(false);
  if (!tasks.length) return null;
  const removable = tasks.filter((t) => finished(t) && !t.active);
  const summary = transferSummary(tasks, transferStore.speed);
  const attentionLabel = `${summary.attention} transfer${summary.attention === 1 ? ' needs' : 's need'} attention`;
  const details = [summary.label];
  if (summary.active)
    details.push(
      summary.percent === null
        ? `${size(summary.bytes)} transferred`
        : `${size(summary.bytes)} / ${size(summary.total)}`,
    );
  if (summary.percent !== null) details.push(`${summary.percent}%`);
  if (summary.speed !== null) details.push(`${size(summary.speed)}/s`);
  if (summary.active && summary.waiting)
    details.push(`${summary.waiting} waiting${summary.attention ? ` (${attentionLabel})` : ''}`);
  const description = details.join(' · ');
  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button
          className={styles.trigger}
          aria-label="File transfers"
          aria-description={description}
          title={description}
        >
          {summary.direction === 'upload' ? (
            <ArrowUpFromLine size={13} />
          ) : summary.direction === 'download' ? (
            <ArrowDownToLine size={13} />
          ) : (
            <ArrowDownUp size={13} />
          )}
          <span className={styles.label}>{summary.label}</span>
          {summary.percent !== null && <span className={styles.percent}>{summary.percent}%</span>}
          {summary.active > 0 && (
            <progress aria-label="File transfer progress" max={100} value={summary.percent ?? undefined} />
          )}
          {summary.speed !== null && <span className={styles.speed}>{size(summary.speed)}/s</span>}
          {summary.attention > 0 ? (
            <span className={styles.attention} aria-label={attentionLabel}>
              <CircleAlert size={12} />
            </span>
          ) : summary.active > 0 && summary.waiting > 0 ? (
            <span title={`${summary.waiting} waiting`}>+{summary.waiting}</span>
          ) : null}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          className={styles.panel}
          side="top"
          align="end"
          sideOffset={8}
          collisionPadding={12}
          aria-label="File transfers"
        >
          <header>
            <strong>Transfers</strong>
            <span className="spacer" />
            <button
              className={styles.clear}
              disabled={clearing || !removable.length}
              onClick={() => {
                setClearing(true);
                void actions
                  .remove(removable.map((t) => t.id))
                  .catch(report)
                  .finally(() => setClearing(false));
              }}
            >
              Clear finished
            </button>
            <Popover.Close asChild>
              <IconButton label="Close transfers">
                <X size={14} />
              </IconButton>
            </Popover.Close>
          </header>
          <div className={styles.list}>
            {tasks.map((task) => (
              <TransferRow key={task.id} task={task} actions={actions} report={report} />
            ))}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
function TransferRow({
  task,
  actions,
  report,
}: {
  task: Transfer;
  actions: TransferActions;
  report: (error: unknown) => void;
}) {
  const [all, setAll] = useState(false);
  const [busy, setBusy] = useState(false);
  const [skipped, setSkipped] = useState<SkippedTransfers | null>(null);
  const [skippedAfter, setSkippedAfter] = useState<number | null>(null);
  const loadSkipped = async (after: number | null) => {
    setSkipped(await call('transfer_skipped', { id: task.id, after }));
    setSkippedAfter(after);
  };
  const run = (action: Promise<unknown>) => {
    setBusy(true);
    void action.catch(report).finally(() => setBusy(false));
  };
  const conflict = task.conflict;
  const choices: [Choice, string][] = conflict
    ? [
        ...(conflict.source === conflict.destination && conflict.source === 'file'
          ? [['replace', 'Replace'] as [Choice, string]]
          : []),
        ...(conflict.source === conflict.destination && conflict.source === 'directory'
          ? [['merge', 'Merge'] as [Choice, string]]
          : []),
        ['keep_both', 'Keep both'],
        ['skip', 'Skip'],
      ]
    : [];
  const location =
    task.direction === 'upload'
      ? `${task.server} · ${task.workspace.replace(/\/$/, '')}/${task.destination}`
      : `${task.server} · ${task.workspace} → ${task.destination}`;
  const label = {
    queued: 'Queued',
    paused: 'Paused',
    preparing: 'Preparing…',
    running: 'Transferring',
    reconnecting: 'Reconnecting…',
    conflict: 'Choose how to continue',
    completed: 'Completed',
    cancelled: 'Cancelled',
  }[task.status];
  const percent = task.total
    ? Math.min(100, Math.floor((task.bytes / task.total) * 100))
    : task.status === 'completed'
      ? 100
      : 0;
  return (
    <article className={styles.task} data-transfer-id={task.id}>
      <div className={styles.heading}>
        {task.direction === 'upload' ? <ArrowUpFromLine size={15} /> : <ArrowDownToLine size={15} />}
        <strong>
          {task.direction === 'upload' ? 'Upload' : 'Download'} · {task.name || task.server}
        </strong>
        <span className="spacer" />
        {!finished(task) && (!task.active || task.owned) && (
          <>
            {task.active ? (
              <IconButton label="Pause transfer" disabled={busy} onClick={() => run(actions.pause(task))}>
                <Pause size={13} />
              </IconButton>
            ) : (
              !conflict && (
                <IconButton label="Resume transfer" disabled={busy} onClick={() => run(actions.resume(task))}>
                  <Play size={13} />
                </IconButton>
              )
            )}
            <IconButton label="Cancel transfer" disabled={busy} onClick={() => run(actions.cancel(task))}>
              <X size={13} />
            </IconButton>
          </>
        )}
        {finished(task) && !task.active && (
          <IconButton label="Remove from history" disabled={busy} onClick={() => run(actions.remove([task.id]))}>
            <Trash2 size={13} />
          </IconButton>
        )}
        {task.direction === 'download' && task.status === 'completed' && (
          <IconButton label="Show in Finder" onClick={() => run(call('transfer_reveal', { id: task.id }))}>
            <FolderOpen size={14} />
          </IconButton>
        )}
      </div>
      <div className={styles.path} title={location}>
        {location}
      </div>
      {!finished(task) && (
        <progress
          aria-label="File transfer progress"
          max={100}
          value={['preparing', 'queued'].includes(task.status) ? undefined : percent}
        />
      )}
      <div className={styles.meta}>
        <span>
          {label}
          {task.active && !task.owned ? ' · Another window' : ''}
        </span>
        <span>
          {size(task.bytes)} / {size(task.total)}
          {task.status === 'running' ? ` · ${size(transferStore.speed(task.id))}/s` : ''}
        </span>
      </div>
      <div className={styles.meta}>
        {task.completed} / {task.files} entries
        {task.skipped > 0 && <button onClick={() => run(loadSkipped(null))}>{task.skipped} skipped</button>}
      </div>
      {task.message && <p className={styles.message}>{task.message}</p>}
      {skipped && (
        <ul className={styles.skipped}>
          {skipped.paths.map((path, index) => (
            <li key={index}>{path}</li>
          ))}
        </ul>
      )}
      {skipped && (
        <div className={styles.meta}>
          {skippedAfter !== null && (
            <button disabled={busy} onClick={() => run(loadSkipped(null))}>
              Back to first entries
            </button>
          )}
          {skipped.next !== null && (
            <button disabled={busy} onClick={() => run(loadSkipped(skipped.next))}>
              Next entries
            </button>
          )}
        </div>
      )}
      {conflict && !task.active && (
        <div className={styles.conflict}>
          <p title={conflict.path}>“{conflict.path}” already exists.</p>
          <div>
            {choices.map(([choice, label]) => (
              <button key={choice} disabled={busy} onClick={() => run(actions.resume(task, choice, all))}>
                {label}
              </button>
            ))}
          </div>
          <label>
            <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />
            Apply to matching conflicts in this transfer
          </label>
        </div>
      )}
      {!task.active &&
        task.status === 'paused' &&
        ['TRANSFER_SOURCE_CHANGED', 'TRANSFER_CHECKSUM', 'TRANSFER_CHECKPOINT', 'TRANSFER_CONFLICT'].includes(
          task.error_code ?? '',
        ) && <button onClick={() => run(actions.resume(task, undefined, false, true))}>Restart file</button>}
    </article>
  );
}
function size(bytes: number) {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let n = Math.max(0, bytes);
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i ? 1 : 0)} ${units[i]}`;
}
