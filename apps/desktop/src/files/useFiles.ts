import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { call, failure, listen, operationId } from '../bridge/client';
import { transferStore } from '../transfers/store';
import { overlaps, remotePath, within } from './context';
import type { FileContext } from '../bridge/files';
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
  const moving = useRef<{ server: string; paths: string[] } | null>(null);
  const roots = useRef(new Map<string, Promise<FileContext>>());
  const relocate = useCallback(
    (server: string, source: string, destination: string) => {
      const keys = store.relocate(server, source, destination);
      setSelected((previous) =>
        Object.fromEntries(Object.entries(previous).map(([id, key]) => [id, keys.get(key) ?? key])),
      );
      return keys;
    },
    [store],
  );
  const bind = useCallback(
    async (key: string) => {
      store.assertWritable(key);
      const tab = store.get(key);
      if (!tab || tab.context) return;
      let request = roots.current.get(tab.server);
      if (!request) {
        request = call('file_context_open', { operationId: operationId(), server: tab.server });
        roots.current.set(tab.server, request);
      }
      try {
        store.bind(key, (await request).id);
      } catch (error) {
        roots.current.delete(tab.server);
        store.locationFailed(key, failure(error).message);
        throw error;
      }
    },
    [store],
  );
  const retry = useCallback(
    async (key: string) => {
      const tab = store.get(key);
      if (tab?.pendingMove) key = relocate(tab.server, tab.pendingMove, tab.pendingMove).get(key) ?? key;
      await bind(key);
      await store.retry(key);
    },
    [bind, store, relocate],
  );
  useEffect(
    () =>
      listen((event) => {
        if (event.kind !== 'file_relocated') return;
        const keys = relocate(event.server, event.source, event.destination);
        for (const key of keys.values())
          void retry(key).catch(() => {
            /* The tab retains its retry state and unsaved text. */
          });
      }),
    [relocate, retry],
  );
  useEffect(() => {
    const retained = store.retainedContexts();
    for (const [server, request] of roots.current)
      void request.then(
        (context) => {
          if (
            retained.has(context.id) ||
            store.retainedContexts().has(context.id) ||
            roots.current.get(server) !== request
          )
            return;
          roots.current.delete(server);
          void call('file_context_close', { id: context.id }).catch(() => {
            /* Window shutdown also releases native contexts. */
          });
        },
        () => {},
      );
  }, [tabs, store]);
  useEffect(
    () => () => {
      for (const request of roots.current.values())
        void request.then(
          (context) => call('file_context_close', { id: context.id }).catch(() => {}),
          () => {},
        );
    },
    [],
  );
  const move = async (context: FileContext, path: string, destination: string) => {
    const paths = [path, destination].map((p) => remotePath(context.path, p));
    store.assertCanMove(context.server, paths[0], paths[1]);
    if (moving.current) throw { message: 'Wait for the current move to finish.' };
    if (
      store
        .buffers()
        .some(
          (b) => b.server === context.server && b.saving && paths.some((p) => overlaps(p, remotePath(b.root, b.path))),
        ) ||
      transferStore
        .snapshot()
        .some(
          (t) =>
            t.active &&
            t.direction === 'upload' &&
            t.server === context.server &&
            paths.some((p) => overlaps(p, remotePath(t.workspace, t.destination))),
        )
    )
      throw { message: 'Wait for saves and uploads in these locations to finish.' };
    moving.current = { server: context.server, paths };
    try {
      await call('file_change', { context: context.id, change: { action: 'rename', path, destination } });
    } finally {
      moving.current = null;
    }
  };

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
      await bind(key);
      store.assertWritable(key);
      const buffer = store.buffers().find((buffer) => buffer.key === key);
      if (!buffer || (buffer.text === buffer.original && !resolve)) return;
      if (
        moving.current?.server === buffer.server &&
        moving.current.paths.some((p) => overlaps(p, remotePath(buffer.root, buffer.path)))
      )
        throw { message: 'Wait for the move to finish before saving this file.' };
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
    [store, update, bind],
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
              !b.pendingMove &&
              b.text === b.original,
          )) {
          void call('file_read', { context: buffer.context, path: buffer.path })
            .then((file) => {
              const latest = store.buffers().find((b) => b.key === buffer.key);
              if (latest && !latest.pendingMove && latest.text === latest.original)
                update(buffer.key, { ...file, original: file.text });
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
    retry,
    move,
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
