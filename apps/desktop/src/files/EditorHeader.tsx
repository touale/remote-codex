import { ArrowLeft, Save, X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { IconButton } from '../ui/controls';
import type { FileTab } from './tabs';
import type { FileChange } from './useFiles';

export function EditorHeader({
  tabs: files,
  active,
  diff,
  onSelect,
  onClose,
  onSave,
  onHide,
  onCloseDiff,
}: {
  tabs: FileTab[];
  active?: FileTab;
  diff: FileChange | null;
  onSelect: (key: string) => void;
  onClose: (key: string) => void;
  onSave: (key: string) => void;
  onHide: () => void;
  onCloseDiff: () => void;
}) {
  const tabs = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const list = tabs.current;
    const selected = list?.querySelector<HTMLElement>('.editor-tab.selected');
    if (!list || !selected) return;
    const bounds = list.getBoundingClientRect();
    const tab = selected.getBoundingClientRect();
    if (tab.left < bounds.left) list.scrollLeft -= bounds.left - tab.left;
    else if (tab.right > bounds.right) list.scrollLeft += tab.right - bounds.right;
  }, [active?.key, diff]);
  return (
    <header className="editor-header" aria-label="Editor tabs and actions">
      {diff ? (
        <div className="diff-heading">
          <IconButton label="Back to editor" onClick={onCloseDiff}>
            <ArrowLeft size={15} />
          </IconButton>
          <span title={diff.path}>Changes · {diff.path.split('/').at(-1)}</span>
        </div>
      ) : active ? (
        <div className="editor-tabs" ref={tabs} role="tablist" aria-label="Open files">
          {files.map((buffer) => (
            <div className={`editor-tab ${buffer.key === active.key ? 'selected' : ''}`} key={buffer.key}>
              <button
                role="tab"
                aria-selected={buffer.key === active.key}
                aria-busy={buffer.status === 'loading'}
                title={`${buffer.server} · ${buffer.root.replace(/\/$/, '')}/${buffer.path}`}
                onClick={() => onSelect(buffer.key)}
              >
                <span className="tab-name">{buffer.path.split('/').at(-1)}</span>
                {buffer.status === 'ready' && buffer.original !== buffer.text && <span className="dirty-dot" />}
              </button>
              <IconButton label={`Close ${buffer.path}`} onClick={() => onClose(buffer.key)}>
                <X size={12} />
              </IconButton>
            </div>
          ))}
        </div>
      ) : null}
      <div className="editor-actions">
        {active && !diff && (
          <IconButton
            label="Save file (⌘S)"
            disabled={
              active.status !== 'ready' ||
              !active.context ||
              !!active.pendingMove ||
              active.saving ||
              active.text === active.original
            }
            onClick={() => onSave(active.key)}
          >
            <Save size={14} />
          </IconButton>
        )}
        <IconButton label="Hide editor" onClick={onHide}>
          <X size={15} />
        </IconButton>
      </div>
    </header>
  );
}
