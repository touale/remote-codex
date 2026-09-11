import { useCallback, useEffect, useRef, useState } from 'react';
import { call, failure, operationId } from '../bridge/client';
import type { FileContext, WorkspaceTarget } from '../bridge/files';
import type { useFiles } from './useFiles';

interface Access {
  target: WorkspaceTarget;
  operation: string;
  task: Promise<FileContext>;
  context?: FileContext;
  error: string | null;
}
export function useDirectoryFiles(
  target: WorkspaceTarget | null,
  files: Pick<ReturnType<typeof useFiles>, 'tabs' | 'retainedContexts'>,
  report: (error: unknown) => void,
  finishOperation: (id: string) => void,
) {
  const cache = useRef(new Map<string, Access>());
  const refs = useRef({ report, finishOperation });
  refs.current = { report, finishOperation };
  const [, update] = useState(0);
  const server = target?.server;
  const path = target?.path;
  const key = target ? JSON.stringify([server, path]) : null;
  const release = useCallback((key: string, entry: Access) => {
    cache.current.delete(key);
    if (entry.context) void call('file_context_close', { id: entry.context.id }).catch(refs.current.report);
    else if (entry.error === null) void call('cancel_operation', { id: entry.operation }).catch(refs.current.report);
  }, []);
  const connect = useCallback((): Promise<FileContext> => {
    if (!key || !server || !path) return Promise.reject(new Error('Choose a directory first.'));
    const cached = cache.current.get(key);
    if (cached && cached.error === null) return cached.task;
    const operation = operationId();
    const entry: Access = {
      target: { server, path },
      operation,
      error: null,
      task: call('file_context_open', { operationId: operation, server, path })
        .then((context) => {
          if (cache.current.get(key) !== entry) {
            void call('file_context_close', { id: context.id }).catch(refs.current.report);
            throw { code: 'OPERATION_CANCELLED', message: 'Directory opening was cancelled.' };
          }
          entry.context = context;
          return context;
        })
        .catch((error) => {
          entry.error = failure(error).message;
          throw error;
        })
        .finally(() => {
          refs.current.finishOperation(operation);
          update((v) => v + 1);
        }),
    };
    cache.current.set(key, entry);
    update((v) => v + 1);
    return entry.task;
  }, [key, server, path]);
  useEffect(() => {
    if (key) void connect().catch(() => {});
  }, [key, connect]);
  // An editor retains its origin context while browsing another directory.
  useEffect(() => {
    const retained = files.retainedContexts();
    for (const [owner, entry] of cache.current)
      if (owner !== key && (!entry.context || !retained.has(entry.context.id))) release(owner, entry);
  }, [key, files.tabs, release]);
  useEffect(
    () => () => {
      for (const [key, entry] of cache.current) release(key, entry);
    },
    [release],
  );
  const current = key ? cache.current.get(key) : undefined;
  return {
    context: current?.context ?? null,
    busy: Boolean(key && !current?.context && !current?.error),
    error: current?.error ?? '',
    connect,
    retry: () => {
      void connect().catch(() => {});
    },
    forget: (server: string, path?: string) => {
      for (const [key, entry] of cache.current)
        if (
          entry.target.server === server &&
          (path === undefined || entry.target.path === path || entry.context?.path === path)
        )
          release(key, entry);
      update((v) => v + 1);
    },
  };
}
