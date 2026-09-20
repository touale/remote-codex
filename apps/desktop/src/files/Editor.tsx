import { lazy, Suspense, useState } from 'react';
import type { MarkdownMode, PreviewView, EditorView } from '../bridge/editor';
import { call, failure } from '../bridge/client';
import { previewType } from './preview';
import type { FileContext } from '../bridge/files';
import { Empty, ErrorText } from '../ui/controls';
import { EditorHeader } from './EditorHeader';
import { EditorSkeleton } from './FileLoading';
import { visibleIn } from './context';
import type { FileTab } from './tabs';
import type { FileChange } from './useFiles';
import { ChangeView } from './ChangeView';
const FileSurface = lazy(() => import('./FileSurface'));

export default function EditorPane({
  context,
  tabs,
  selected,
  dark,
  diff,
  onSelect,
  onClose,
  onSave,
  onChange,
  onHide,
  onRetry,
  onCloseDiff,
  onDismissConflict,
  onDiscardConflict,
  onWindow,
  onDiffWindow,
  onMode,
  onPreviewView,
  onOpenFile,
}: {
  context: FileContext | null;
  tabs: FileTab[];
  selected?: string;
  dark: boolean;
  diff: FileChange | null;
  onSelect: (key: string) => void;
  onClose: (key: string) => void;
  onSave: (key: string, resolve?: boolean) => void;
  onChange: (key: string, text: string) => void;
  onHide: () => void;
  onRetry: (key: string) => void;
  onCloseDiff: () => void;
  onDismissConflict: (key: string) => void;
  onDiscardConflict: (key: string) => void;
  onWindow?: (key: string) => void;
  onDiffWindow?: () => void;
  onMode: (key: string, mode: MarkdownMode, view?: EditorView) => void;
  onPreviewView: (key: string, view: PreviewView) => void;
  onOpenFile: (context: string, path: string, server: string, root: string) => Promise<void>;
}) {
  const [downloadError, setDownloadError] = useState<{ key: string; message: string } | null>(null);
  const visible = tabs.filter((buffer) => visibleIn(context, buffer));
  const active = visible.find((buffer) => buffer.key === selected) ?? visible.at(-1);
  return (
    <section className="editor-pane" aria-label="File editor">
      <EditorHeader
        tabs={visible}
        active={active}
        diff={diff}
        onSelect={onSelect}
        onClose={onClose}
        onSave={onSave}
        onHide={onHide}
        onCloseDiff={onCloseDiff}
        onWindow={onWindow}
        onDiffWindow={onDiffWindow}
      />
      {diff ? (
        <ChangeView diff={diff.diff} />
      ) : active?.status === 'loading' ? (
        <EditorSkeleton />
      ) : active?.status === 'failed' ? (
        <Empty title="Could not open file">
          <ErrorText message={active.locationError ?? active.error} />
          <ErrorText message={downloadError?.key === active.key ? downloadError.message : undefined} />
          <div className="actions">
            {previewType(active.path) && (
              <button
                onClick={() => {
                  void call('transfer_download', { context: active.context, source: active.path })
                    .then((task) => task && call('transfer_run', { id: task.id }))
                    .catch((error) => setDownloadError({ key: active.key, message: failure(error).message }));
                }}
              >
                Download
              </button>
            )}
            <button onClick={() => onClose(active.key)}>Close file</button>
            <button onClick={() => onRetry(active.key)}>Retry</button>
          </div>
        </Empty>
      ) : active?.status === 'ready' ? (
        <Suspense fallback={<EditorSkeleton />}>
          <FileSurface
            key={active.key}
            onMode={onMode}
            onPreviewView={onPreviewView}
            onOpenFile={(path) => onOpenFile(active.context, path, active.server, active.root)}
            active={active}
            onRetry={onRetry}
            dark={dark}
            onSave={onSave}
            onChange={onChange}
            onDismissConflict={onDismissConflict}
            onDiscardConflict={onDiscardConflict}
          />
        </Suspense>
      ) : null}
    </section>
  );
}
