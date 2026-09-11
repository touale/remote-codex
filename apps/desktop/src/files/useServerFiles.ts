import { useEffect, useRef, useState } from 'react';
import { call, failure, operationId } from '../bridge/client';
import type { FileContext } from '../bridge/files';
import type { Server } from '../bridge/types';
import type { useFiles } from './useFiles';

export function useServerFiles(
  server: Server | null,
  files: Pick<ReturnType<typeof useFiles>, 'tabs' | 'retainedContexts'>,
  report: (error: unknown) => void,
  finishOperation: (id: string) => void,
) {
  const cache = useRef(new Map<string, FileContext>());
  const refs = useRef({ report, finishOperation });
  refs.current = { report, finishOperation };
  const [attempt, retry] = useState(0);
  const [result, setResult] = useState<{
    server: string;
    context: FileContext | null;
    busy: boolean;
    error: string;
  } | null>(null);
  const name = server?.name;
  useEffect(() => {
    if (!name) return;
    const cached = cache.current.get(name);
    let cancelled = false;
    const operation = operationId();
    const close = (context: FileContext) =>
      void call('file_context_close', { id: context.id }).catch(refs.current.report);
    if (cached) setResult({ server: name, context: cached, busy: false, error: '' });
    else {
      setResult({ server: name, context: null, busy: true, error: '' });
      void call('file_context_open', { operationId: operation, server: name })
        .then((context) => {
          if (cancelled) close(context);
          else {
            cache.current.set(name, context);
            setResult({ server: name, context, busy: false, error: '' });
          }
        })
        .catch((error) => {
          if (!cancelled) setResult({ server: name, context: null, busy: false, error: failure(error).message });
        })
        .finally(() => refs.current.finishOperation(operation));
    }
    return () => {
      cancelled = true;
      if (!cached) void call('cancel_operation', { id: operation }).catch(refs.current.report);
    };
  }, [name, attempt]);
  // Keep an origin handle alive while any editor buffer uses it, even in another view.
  useEffect(() => {
    const retained = files.retainedContexts();
    for (const [owner, context] of cache.current) {
      if (owner === name || retained.has(context.id)) continue;
      cache.current.delete(owner);
      void call('file_context_close', { id: context.id }).catch(report);
    }
  }, [name, files.tabs, report]);
  const current =
    name && result?.server === name && (!result.context || cache.current.get(name)?.id === result.context.id)
      ? result
      : null;
  return {
    context: current?.context ?? null,
    busy: Boolean(name && (current?.busy ?? true)),
    error: current?.error ?? '',
    retry: () => retry((v) => v + 1),
    forget: (server: string, path?: string) => {
      if (path !== undefined && cache.current.get(server)?.path !== path) return;
      cache.current.delete(server);
      setResult((r) =>
        r?.server === server
          ? { server, context: null, busy: false, error: 'Files are disconnected. Retry to reconnect.' }
          : r,
      );
    },
  };
}
