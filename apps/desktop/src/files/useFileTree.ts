import { useCallback, useEffect, useRef, useState } from 'react';
import { call, failure, listen } from '../bridge/client';
import type { DirectoryPage } from '../bridge/types';
import { overlaps, remotePath } from './context';
interface Tree {
  pages: Record<string, DirectoryPage>;
  expanded: Set<string>;
  states: Record<string, { status: 'loading' | 'ready' } | { status: 'failed'; error: string }>;
}
const emptyTree = (expanded: string[] = []): Tree => ({ pages: {}, expanded: new Set(expanded), states: {} });
export function useFileTree(
  context: string | null,
  cacheKey: string,
  refresh: number,
  root: string | null,
  server: string | null,
) {
  const cache = useRef(new Map<string, Tree>());
  const requests = useRef(new Map<string, symbol>());
  const active = useRef(cacheKey);
  active.current = cacheKey;
  const [snapshot, setSnapshot] = useState<{ key: string; tree: Tree }>({
    key: cacheKey,
    tree: emptyTree(),
  });
  const setTree = (tree: Tree) => setSnapshot({ key: cacheKey, tree });
  const tree = snapshot.key === cacheKey ? snapshot.tree : (cache.current.get(cacheKey) ?? emptyTree());
  const currentContext = useRef(context);
  currentContext.current = context;
  const load = useCallback(
    async (path: string) => {
      if (!context || active.current !== cacheKey || currentContext.current !== context) return;
      const token = Symbol(path);
      requests.current.set(path, token);
      const publish = (reduce: (tree: Tree) => Tree) => {
        if (requests.current.get(path) !== token || active.current !== cacheKey || currentContext.current !== context)
          return;
        const next = reduce(cache.current.get(cacheKey) ?? emptyTree());
        cache.current.set(cacheKey, next);
        setSnapshot({ key: cacheKey, tree: next });
      };
      publish((previous) => ({ ...previous, states: { ...previous.states, [path]: { status: 'loading' } } }));
      try {
        const page = await call('file_list', { context, path });
        publish((previous) => ({
          ...previous,
          pages: { ...previous.pages, [path]: page },
          states: { ...previous.states, [path]: { status: 'ready' } },
        }));
      } catch (error) {
        publish((previous) => ({
          ...previous,
          states: { ...previous.states, [path]: { status: 'failed', error: failure(error).message } },
        }));
      } finally {
        if (requests.current.get(path) === token) requests.current.delete(path);
      }
    },
    [context, cacheKey],
  );
  useEffect(() => {
    let saved: string[] = [];
    try {
      const value: unknown = JSON.parse(localStorage.getItem(`file-tree:${cacheKey}`) ?? '[]');
      if (Array.isArray(value)) saved = value.filter((p): p is string => typeof p === 'string').slice(0, 128);
    } catch {
      /* An invalid UI preference does not block a workspace. */
    }
    const snapshot = cache.current.get(cacheKey) ?? emptyTree(saved);
    cache.current.set(cacheKey, snapshot);
    setTree(snapshot);
    let cancelled = false;
    const paths = ['', ...snapshot.expanded];
    const worker = async () => {
      while (!cancelled && paths.length) {
        const path = paths.shift();
        if (path !== undefined && (!path || cache.current.get(cacheKey)?.expanded.has(path))) await load(path);
      }
    };
    void Promise.all(Array.from({ length: 4 }, worker));
    return () => {
      cancelled = true;
      requests.current.clear();
      const latest = cache.current.get(cacheKey);
      if (latest)
        cache.current.set(cacheKey, {
          ...latest,
          states: Object.fromEntries(Object.entries(latest.states).filter(([, state]) => state.status !== 'loading')),
        });
    };
  }, [cacheKey, load, refresh]);
  useEffect(() => {
    const refreshed = new Set<string>();
    return listen((event) => {
      if (event.kind !== 'transfer') return;
      const t = event.transfer;
      if (
        t.direction !== 'upload' ||
        !root ||
        server !== t.server ||
        !['completed', 'cancelled'].includes(t.status) ||
        refreshed.has(t.id)
      )
        return;
      refreshed.add(t.id);
      const snapshot = cache.current.get(cacheKey);
      const destination = remotePath(t.workspace, t.destination);
      const paths = new Set(
        ['', ...(snapshot?.expanded ?? [])].filter((path) => overlaps(remotePath(root, path), destination)),
      );
      void (async () => {
        for (const path of paths) await load(path);
      })();
    });
  }, [cacheKey, load, root, server]);
  const toggle = async (path: string) => {
    const previous = cache.current.get(cacheKey) ?? tree;
    const expanded = new Set(previous.expanded);
    const opening = !expanded.has(path);
    if (opening) expanded.add(path);
    else expanded.delete(path);
    const next = { ...previous, expanded };
    cache.current.set(cacheKey, next);
    setTree(next);
    localStorage.setItem(`file-tree:${cacheKey}`, JSON.stringify([...expanded]));
    if (opening && !previous.pages[path]) await load(path);
  };
  const reveal = async (path: string) => {
    const previous = cache.current.get(cacheKey) ?? tree;
    const expanded = new Set(previous.expanded);
    if (path) expanded.add(path);
    const next = { ...previous, expanded };
    cache.current.set(cacheKey, next);
    localStorage.setItem(`file-tree:${cacheKey}`, JSON.stringify([...expanded]));
    if (active.current === cacheKey && currentContext.current === context) setTree(next);
    await load(path);
  };
  return {
    ...tree,
    load,
    toggle,
    reveal,
  };
}
