import { Save } from 'lucide-react';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { Tooltip } from 'radix-ui';
import { call, failure, operationId } from '../bridge/client';
import type { ContentTarget } from '../bridge/editor';
import type { FileContext } from '../bridge/files';
import { ChangeView } from '../files/ChangeView';
import { EditorSkeleton } from '../files/FileLoading';
import { useFiles } from '../files/useFiles';
import { useFileActions } from '../files/useFileActions';
import { Authentication } from '../settings/Authentication';
import { Empty, ErrorText, IconButton } from '../ui/controls';
import { useDialog } from '../ui/useDialog';
import type { useApplication } from './useApplication';
import styles from './App.module.css';
import './content-window.css';
const EditorSurface = lazy(() => import('../files/EditorSurface'));

export function ContentWindow({
  app,
  target,
}: {
  app: Pick<
    ReturnType<typeof useApplication>,
    'dark' | 'prompts' | 'answer' | 'report' | 'error' | 'setError' | 'closeHandler'
  >;
  target: ContentTarget;
}) {
  const files = useFiles();
  const dialog = useDialog();
  const [context, setContext] = useState<FileContext | null>(null);
  const [ready, setReady] = useState(target.kind === 'diff');
  const started = useRef(false);
  const closing = useRef(false);
  const actions = useFileActions(
    context,
    files,
    dialog.ask,
    () => {},
    () => {},
  );
  const active = files.tabs.find((file) => file.status === 'ready');
  const failed = files.tabs.find((file) => file.status === 'failed');
  const run = (task: Promise<unknown>) => void task.catch(app.report);
  useEffect(() => {
    if (started.current || target.kind !== 'file') return;
    started.current = true;
    const operation = operationId();
    void (async () => {
      const doc = target.document;
      const context = await call('file_context_open', { operationId: operation, server: doc.server, path: doc.root });
      setContext(context);
      if (target.transfer) {
        files.adopt(context.id, doc);
        await call('editor_window_ready', { error: null });
      } else {
        await files.open(context.id, doc.path, doc.server, context.path);
      }
      setReady(true);
    })().catch(async (error) => {
      app.report(error);
      if (target.transfer) await call('editor_window_ready', { error: failure(error).message }).catch(() => {});
    });
  }, [target]);
  const close = async () => {
    if (closing.current) return;
    closing.current = true;
    try {
      if (ready && active && !(await actions.close(active.key))) {
        await call('close_window', { cancel: true });
        return;
      }
      await call('close_window', { cancel: false });
    } catch (error) {
      await call('close_window', { cancel: true });
      app.report(error);
    } finally {
      closing.current = false;
    }
  };
  app.closeHandler.current = () => run(close());
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (
        (event.metaKey || event.ctrlKey) &&
        event.key.toLowerCase() === 's' &&
        ready &&
        active &&
        !document.querySelector('[role=dialog]')
      ) {
        event.preventDefault();
        run(actions.save(active.key));
      }
    };
    document.addEventListener('keydown', key, true);
    return () => document.removeEventListener('keydown', key, true);
  });
  const doc = target.document;
  return (
    <Tooltip.Provider delayDuration={400}>
      <main className={`${styles.app} content-window`}>
        <header className={styles.titlebar} data-tauri-drag-region>
          <div className={styles.trafficSpace} data-tauri-drag-region />
          <div
            className="content-title"
            data-tauri-drag-region
            title={`${doc.server} · ${doc.root}/${active?.path ?? doc.path}`}
          >
            <strong data-tauri-drag-region>
              {target.kind === 'diff' ? 'Changes · ' : ''}
              {(active?.path ?? doc.path).split('/').at(-1)}
            </strong>
            <small data-tauri-drag-region>
              {doc.server} · {doc.path}
            </small>
          </div>
          <div className={styles.dragSpace} data-tauri-drag-region />
          {target.kind === 'file' && (
            <IconButton
              label="Save file (⌘S)"
              disabled={!ready || !active || active.saving || active.text === active.original}
              onClick={() => {
                if (active) run(actions.save(active.key));
              }}
            >
              <Save size={16} />
            </IconButton>
          )}
        </header>
        {app.error && (
          <div className="content-error">
            <ErrorText message={app.error} />
          </div>
        )}
        {target.kind === 'diff' ? (
          <ChangeView diff={target.document.diff} standalone />
        ) : failed ? (
          <Empty title="Could not open file">
            <ErrorText message={failed.error} />
            <button onClick={() => run(files.retry(failed.key))}>Retry</button>
          </Empty>
        ) : !ready || !active ? (
          <EditorSkeleton />
        ) : (
          <Suspense fallback={<EditorSkeleton />}>
            <EditorSurface
              active={active}
              dark={app.dark}
              onRetry={(key) => run(files.retry(key))}
              onSave={(key, resolve) => run(actions.save(key, resolve))}
              onChange={(key, text) => files.update(key, { text })}
              onDismissConflict={(key) => files.update(key, { conflict: undefined })}
              onDiscardConflict={(key) => {
                files.discard(key);
                app.setError('');
              }}
            />
          </Suspense>
        )}
        {dialog.dialog}
        {app.prompts[0] && <Authentication key={app.prompts[0].id} request={app.prompts[0]} onAnswer={app.answer} />}
      </main>
    </Tooltip.Provider>
  );
}
