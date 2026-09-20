import { useEffect, useRef, useState } from 'react';
import { Tooltip } from 'radix-ui';
import { call, failure, operationId } from '../bridge/client';
import type { ContentTarget } from '../bridge/editor';
import type { FileContext } from '../bridge/files';
import { ChangeView } from '../files/ChangeView';
import { EditorSkeleton } from '../files/FileLoading';
import { useFiles } from '../files/useFiles';
import { useFileActions } from '../files/useFileActions';
import { Authentication } from '../settings/Authentication';
import { ErrorText } from '../ui/controls';
import { useDialog } from '../ui/useDialog';
import type { useApplication } from './useApplication';
import styles from './App.module.css';
import './content-window.css';
import EditorPane from '../files/Editor';
import { fileKey, isText } from '../files/tabs';

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
  const started = useRef(false);
  const closing = useRef(false);
  const actions = useFileActions(
    context,
    files,
    dialog.ask,
    () => {},
    () => {},
  );
  const active = files.tabs.find((file) => file.key === files.selected[context?.id ?? '']) ?? files.tabs.at(-1);
  const run = (task: Promise<unknown>) => void task.catch(app.report);
  useEffect(() => {
    if (started.current || target.kind !== 'file') return;
    started.current = true;
    const operation = operationId();
    void (async () => {
      const doc = target.document;
      const context = await call('file_context_open', { operationId: operation, server: doc.server, path: doc.root });
      setContext(context);
      if (target.transfer && doc.kind === 'text') {
        files.adopt(context.id, doc);
      } else {
        await files.open(context.id, doc.path, doc.server, context.path);
        const tab = files.allTabs().find((tab) => tab.key === fileKey(doc.server, context.path, doc.path));
        if (tab?.status !== 'ready')
          throw new Error(tab?.status === 'failed' ? tab.error : 'File could not be opened.');
        if (doc.kind === 'preview' && doc.previewView) files.setPreviewView(tab.key, doc.previewView);
        if (doc.kind === 'text' && doc.mode) files.update(tab.key, { mode: doc.mode });
      }
      if (target.transfer) await call('editor_window_ready', { error: null });
    })().catch(async (error) => {
      app.report(error);
      if (target.transfer) await call('editor_window_ready', { error: failure(error).message }).catch(() => {});
    });
  }, [target]);
  const close = async () => {
    if (closing.current) return;
    closing.current = true;
    try {
      for (const tab of files.allTabs()) {
        if (!(await actions.close(tab.key))) {
          await call('close_window', { cancel: true });
          return;
        }
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
        active &&
        isText(active) &&
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
        </header>
        {app.error && (
          <div className="content-error">
            <ErrorText message={app.error} />
          </div>
        )}
        {target.kind === 'diff' ? (
          <ChangeView diff={target.document.diff} standalone />
        ) : !context ? (
          <EditorSkeleton />
        ) : (
          <EditorPane
            context={context}
            tabs={files.tabs}
            selected={files.selected[context.id]}
            dark={app.dark}
            diff={null}
            onSelect={(key) => files.select(context.id, key)}
            onClose={(key) => run(actions.close(key))}
            onHide={() => run(close())}
            onCloseDiff={() => {}}
            onRetry={(key) => run(files.retry(key))}
            onSave={(key, resolve) => run(actions.save(key, resolve))}
            onChange={(key, text) => files.update(key, { text })}
            onMode={(key, mode, view) => files.update(key, { mode, ...(view ? { view } : {}) })}
            onPreviewView={files.setPreviewView}
            onOpenFile={files.open}
            onDismissConflict={(key) => files.update(key, { conflict: undefined })}
            onDiscardConflict={(key) => {
              files.discard(key);
              app.setError('');
            }}
          />
        )}
        {dialog.dialog}
        {app.prompts[0] && <Authentication key={app.prompts[0].id} request={app.prompts[0]} onAnswer={app.answer} />}
      </main>
    </Tooltip.Provider>
  );
}
