import { useRef } from 'react';
import { call, failure, operationId } from '../bridge/client';
import type { FileDocument } from '../bridge/editor';
import { captureView } from './editorView';
import type { FileTabs } from './tabs';

export function useFileWindows(store: FileTabs) {
  const pending = useRef(new Map<string, Promise<void>>());
  const moveWindow = (key: string): Promise<void> => {
    const buffer = store.buffer(key);
    if (!buffer || buffer.saving || buffer.pendingMove || !buffer.context)
      return Promise.reject(new Error('Wait for this file to finish opening or saving.'));
    if (buffer.transferring) return pending.current.get(buffer.transferring) ?? Promise.resolve();
    const transfer = operationId();
    const document: FileDocument = {
      server: buffer.server,
      root: buffer.root,
      path: buffer.path,
      text: buffer.text,
      original: buffer.original,
      revision: buffer.revision,
      view: captureView(key),
    };
    store.update(key, { transferring: transfer });
    const task = call('new_window', { target: { kind: 'file', context: buffer.context, transfer, document } })
      .then(() => {
        const latest = store.buffers().find((b) => b.transferring === transfer);
        if (latest) store.close(latest.key);
      })
      .catch((error) => {
        if (failure(error).code !== 'TRANSFER_CANCELLED') throw error;
      })
      .finally(() => {
        pending.current.delete(transfer);
        const latest = store.buffers().find((b) => b.transferring === transfer);
        if (latest) store.update(latest.key, { transferring: undefined });
      });
    pending.current.set(transfer, task);
    return task;
  };
  const cancelTransfers = async () => {
    await Promise.all(
      [...pending.current].map(async ([transfer, task]) => {
        await call('editor_window_cancel', { transfer });
        await task.catch(() => {});
      }),
    );
  };
  return { moveWindow, cancelTransfers };
}
