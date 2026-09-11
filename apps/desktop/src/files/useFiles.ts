import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { call, failure, listen } from '../bridge/client';
import { transferStore } from '../transfers/store';
import { remotePath, within } from './context';
import { FileTabs, fileKey } from './tabs';
export interface FileChange {
  path: string;
  diff: string;
  workspace?: string;
}
export function useFiles() {
  const [store] = useState(() => new FileTabs((context, path) => call('file_read', { context, path })));
  const tabs = useSyncExternalStore(store.subscribe, store.snapshot);
  const commits = useRef(new Map<string, Promise<void>>());
  const [selected, setSelected] = useState<Record<string, string>>({});
  const [diff, setDiff] = useState<FileChange | null>(null);
  const update = store.update;
  const open = useCallback(
    (context: string, path: string, server: string, root: string) => {
      const key = fileKey(server, root, path);
      setSelected((previous) => ({ ...previous, [context]: key }));
      setDiff(null);
      return store.open({ key, context, path, server, root });
    },
    [store],
  );
  const write = useCallback(
    async (key: string, resolve = false) => {
      const buffer = store.buffers().find((buffer) => buffer.key === key);
      if (!buffer || (buffer.text === buffer.original && !resolve)) return;
      if (transferStore.locked(buffer.server, buffer.root, buffer.path))
        throw { message: 'Wait for the upload to finish before saving this file.' };
      update(key, { saving: true });
      try {
        const revision = await call('file_write', {
          context: buffer.context,
          path: buffer.path,
          text: buffer.text,
          revision: resolve ? buffer.conflict?.revision : buffer.revision,
        });
        update(key, { revision, original: buffer.text, conflict: undefined });
      } catch (error) {
        if (failure(error).code === 'FILE_CONFLICT') {
          const remote = await call('file_read', { context: buffer.context, path: buffer.path });
          update(key, { conflict: remote });
        }
        throw error;
      } finally {
        update(key, { saving: false });
      }
    },
    [store, update],
  );
  const save = useCallback(
    async (key: string, resolve = false) => {
      const previous = commits.current.get(key);
      const next = (async () => {
        if (previous) await previous;
        await write(key, resolve);
      })();
      commits.current.set(key, next);
      try {
        await next;
      } finally {
        if (commits.current.get(key) === next) commits.current.delete(key);
      }
    },
    [write],
  );
  const get = useCallback((key: string) => store.buffers().find((buffer) => buffer.key === key), [store]);
  const all = store.buffers;
  const discard = useCallback(
    (key: string) => {
      const buffer = store.buffers().find((buffer) => buffer.key === key);
      if (buffer?.conflict) update(key, { ...buffer.conflict, original: buffer.conflict.text, conflict: undefined });
    },
    [store, update],
  );
  const close = store.close;
  const select = useCallback((context: string, key: string) => {
    setSelected((previous) => ({ ...previous, [context]: key }));
    setDiff(null);
  }, []);
  useEffect(
    () =>
      listen((event) => {
        if (event.kind !== 'transfer') return;
        const t = event.transfer;
        if (t.active || t.direction !== 'upload') return;
        for (const buffer of store
          .buffers()
          .filter(
            (b) =>
              b.server === t.server &&
              within(remotePath(t.workspace, t.destination), remotePath(b.root, b.path)) &&
              b.text === b.original,
          )) {
          void call('file_read', { context: buffer.context, path: buffer.path })
            .then((file) => {
              const latest = store.buffers().find((b) => b.key === buffer.key);
              if (latest && latest.text === latest.original) update(buffer.key, { ...file, original: file.text });
            })
            .catch(() => {
              /* Existing revision checks still protect a subsequent save. */
            });
        }
      }),
    [store, update],
  );
  return {
    get,
    all,
    discard,
    tabs,
    allTabs: store.snapshot,
    retry: store.retry,
    selected,
    diff,
    setDiff,
    update,
    open,
    save,
    close,
    select,
    retainedContexts: store.retainedContexts,
  };
}
