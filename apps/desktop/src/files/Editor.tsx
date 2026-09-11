import { lazy, Suspense } from 'react';
import type { FileContext } from '../bridge/files';
import { Empty, ErrorText } from '../ui/controls';
import { EditorHeader } from './EditorHeader';
import { EditorSkeleton } from './FileLoading';
import { visibleIn } from './context';
import type { FileTab } from './tabs';
import type { FileChange } from './useFiles';
const EditorSurface = lazy(() => import('./EditorSurface'));

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
}) {
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
      />
      {diff ? (
        <>
          <pre className="patch-view">
            {diff.diff.split('\n').map((line, index) => (
              <div key={index} className={line.startsWith('+') ? 'addition' : line.startsWith('-') ? 'deletion' : ''}>
                {line || ' '}
              </div>
            ))}
          </pre>
        </>
      ) : active?.status === 'loading' ? (
        <EditorSkeleton />
      ) : active?.status === 'failed' ? (
        <Empty title="Could not open file">
          <ErrorText message={active.locationError ?? active.error} />
          <div className="actions">
            <button onClick={() => onClose(active.key)}>Close file</button>
            <button onClick={() => onRetry(active.key)}>Retry</button>
          </div>
        </Empty>
      ) : active?.status === 'ready' ? (
        <Suspense fallback={<EditorSkeleton />}>
          <EditorSurface
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
