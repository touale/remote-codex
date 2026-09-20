import { lazy, Suspense, useCallback, useEffect, useState } from 'react';
import type { EditorView, MarkdownMode, PreviewView } from '../bridge/editor';
import { call, failure } from '../bridge/client';
import { captureView } from './editorView';
import { EditorSkeleton } from './FileLoading';
import { MarkdownPreview } from './MarkdownPreview';
import { isMarkdown } from './preview';
import type { ReadyFile } from './tabs';
import { ResizeHandle } from '../ui/ResizeHandle';
import { Download, RotateCw } from 'lucide-react';
import { ErrorText, IconButton } from '../ui/controls';
import './preview.css';
const EditorSurface = lazy(() => import('./EditorSurface'));
const ImagePreview = lazy(() => import('./ImagePreview'));
const PdfPreview = lazy(() => import('./PdfPreview'));

export default function FileSurface({
  active,
  dark,
  onSave,
  onRetry,
  onChange,
  onDismissConflict,
  onDiscardConflict,
  onMode,
  onPreviewView,
  onOpenFile,
}: {
  active: ReadyFile;
  dark: boolean;
  onSave: (key: string, resolve?: boolean) => void;
  onRetry: (key: string) => void;
  onChange: (key: string, text: string) => void;
  onDismissConflict: (key: string) => void;
  onDiscardConflict: (key: string) => void;
  onMode: (key: string, mode: MarkdownMode, view?: EditorView) => void;
  onPreviewView: (key: string, view: PreviewView) => void;
  onOpenFile: (path: string) => Promise<void>;
}) {
  const [error, setError] = useState('');
  const [sourceWidth, setSourceWidth] = useState<number | null>(null);
  const [panes, setPanes] = useState<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    if (!panes) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(panes);
    return () => observer.disconnect();
  }, [panes]);
  const report = useCallback((error: unknown) => setError(failure(error).message), []);
  const binary = 'preview' in active;
  const mode = !binary ? (active.mode ?? 'edit') : 'preview';
  const markdown = !binary && isMarkdown(active.path);
  const actions = (
    <>
      <IconButton label="Reload preview" disabled={!!active.transferring} onClick={() => onRetry(active.key)}>
        <RotateCw size={14} />
      </IconButton>
      <IconButton
        label="Download file"
        onClick={() => {
          void call('transfer_download', { context: active.context, source: active.path })
            .then((task) => {
              if (task) return call('transfer_run', { id: task.id });
            })
            .catch(report);
        }}
      >
        <Download size={14} />
      </IconButton>
    </>
  );
  return (
    <div className="file-surface">
      {(error || active.locationError) && <ErrorText message={error || active.locationError} />}
      {markdown && (
        <div className="preview-toolbar" role="group" aria-label="Markdown view">
          {(['edit', 'preview', 'split'] as const).map((choice) => (
            <button
              key={choice}
              aria-pressed={mode === choice}
              disabled={!!active.transferring}
              onClick={() => onMode(active.key, choice, captureView(active.key))}
            >
              {choice === 'edit' ? 'Edit' : choice === 'preview' ? 'Preview' : 'Split'}
            </button>
          ))}
        </div>
      )}
      <Suspense fallback={<EditorSkeleton />}>
        {binary ? (
          active.preview.format === 'pdf' ? (
            <PdfPreview file={active} actions={actions} onView={(view) => onPreviewView(active.key, view)} />
          ) : (
            <ImagePreview file={active} actions={actions} onView={(view) => onPreviewView(active.key, view)} />
          )
        ) : (
          <div className="markdown-panes" ref={setPanes}>
            {(!markdown || mode !== 'preview') && (
              <div
                className="markdown-source"
                style={
                  markdown && mode === 'split'
                    ? {
                        flex: 'none',
                        width: sourceWidth === null ? '50%' : Math.max(120, Math.min(sourceWidth, width - 120)),
                      }
                    : undefined
                }
              >
                <EditorSurface
                  active={active}
                  dark={dark}
                  onSave={onSave}
                  onRetry={onRetry}
                  onChange={onChange}
                  onDismissConflict={onDismissConflict}
                  onDiscardConflict={onDiscardConflict}
                />
              </div>
            )}
            {markdown && mode === 'split' && width > 0 && (
              <ResizeHandle
                axis="x"
                label="Markdown split"
                value={sourceWidth ?? width / 2}
                min={120}
                max={Math.max(120, width - 120)}
                onChange={setSourceWidth}
              />
            )}
            {markdown && mode !== 'edit' && <MarkdownPreview file={active} onOpenFile={onOpenFile} report={report} />}
          </div>
        )}
      </Suspense>
    </div>
  );
}
