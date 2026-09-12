import { useCallback, useRef, useState } from 'react';
import { call, failure, operationId } from '../bridge/client';
import type { CurrentWorkspace } from '../bridge/files';
import type { useApplication } from './useApplication';

interface WorkspaceAccess {
  server: string;
  path: string;
  task: Promise<CurrentWorkspace>;
  value?: CurrentWorkspace;
  error: string | null;
}
const workspaceKey = (server: string, path: string) => JSON.stringify([server, path]);

// Drafts, session preparation and terminals share one request for each workspace.
export function useWorkspaceAccess(app: Pick<ReturnType<typeof useApplication>, 'report'>) {
  const entries = useRef(new Map<string, WorkspaceAccess>());
  const callbacks = useRef(app);
  callbacks.current = app;
  const [, update] = useState(0);
  const connect = useCallback((server: string, path: string) => {
    const key = workspaceKey(server, path);
    const cached = entries.current.get(key);
    if (cached && cached.error === null) return cached.task;
    const operation = operationId();
    const entry: WorkspaceAccess = {
      server,
      path,
      error: null,
      task: call('workspace_open', { operationId: operation, server, path })
        .then((value) => {
          if (entries.current.get(key) !== entry) {
            void call('workspace_close', { id: value.id }).catch(callbacks.current.report);
            throw { code: 'OPERATION_CANCELLED', message: 'The workspace was removed while opening.' };
          }
          entry.value = value;
          const canonical = workspaceKey(value.server, value.path);
          // Keep a concurrent request for another spelling owned by its caller.
          if (!entries.current.has(canonical)) entries.current.set(canonical, entry);
          return value;
        })
        .catch((error) => {
          const issue = failure(error);
          entry.error =
            issue.code === 'OPERATION_CANCELLED' ? 'Connection cancelled. Retry to load files.' : issue.message;
          throw error;
        })
        .finally(() => {
          update((value) => value + 1);
        }),
    };
    entries.current.set(key, entry);
    update((value) => value + 1);
    return entry.task;
  }, []);
  return {
    connect,
    get: (server: string, path: string) => entries.current.get(workspaceKey(server, path)),
    forget: (server: string, path?: string) => {
      for (const [key, entry] of entries.current)
        if (entry.server === server && (path === undefined || entry.path === path || entry.value?.path === path))
          entries.current.delete(key);
      update((value) => value + 1);
    },
  };
}
