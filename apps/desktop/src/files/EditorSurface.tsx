import Editor, { DiffEditor, loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import EditorWorker from 'monaco-editor/editor/editor.worker?worker';
import { useEffect, useState } from 'react';
import { registerEditor } from './editorView';
import { useTransferLock } from '../transfers/store';
import { ErrorText, Modal } from '../ui/controls';
import { EditorSkeleton } from './FileLoading';
import { fileUri } from './context';
import type { Buffer } from './tabs';
self.MonacoEnvironment = { getWorker: () => new EditorWorker() };
loader.config({ monaco });
const options: monaco.editor.IStandaloneEditorConstructionOptions = {
  minimap: { enabled: false },
  fontSize: 13,
  fontFamily: 'SFMono-Regular, Menlo, monospace',
  scrollBeyondLastLine: false,
  automaticLayout: true,
  padding: { top: 12 },
  lineNumbersMinChars: 3,
  renderLineHighlight: 'none',
  overviewRulerLanes: 0,
  fixedOverflowWidgets: true,
  scrollbar: { verticalScrollbarSize: 7, horizontalScrollbarSize: 7, useShadows: false },
};
export default function EditorSurface({
  active,
  dark,
  onSave,
  onRetry,
  onChange,
  onDismissConflict,
  onDiscardConflict,
}: {
  active: Buffer;
  onRetry: (key: string) => void;
  dark: boolean;
  onSave: (key: string, resolve?: boolean) => void;
  onChange: (key: string, text: string) => void;
  onDismissConflict: (key: string) => void;
  onDiscardConflict: (key: string) => void;
}) {
  const [editor, setEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
  useEffect(() => (editor ? registerEditor(active.key, editor, active.view) : undefined), [editor, active.key]);
  const uploading = useTransferLock(active.server, active.root, active.path);
  useEffect(() => {
    for (const [name, base, background, foreground, border] of [
      ['remote-light', 'vs', '#FFFFFF', '#0D0D0D', '#E5E5E5'],
      ['remote-dark', 'vs-dark', '#212121', '#ECECEC', '#3B3B3B'],
    ]) {
      monaco.editor.defineTheme(name, {
        base: base as 'vs' | 'vs-dark',
        inherit: true,
        rules: [],
        colors: {
          'editor.background': background,
          'editor.foreground': foreground,
          'editorLineNumber.foreground': dark ? '#777777' : '#999999',
          'editorWidget.background': background,
          'editorWidget.border': border,
          'editorGutter.background': background,
          'scrollbarSlider.background': base === 'vs-dark' ? '#a3a3a373' : '#66666673',
          'scrollbarSlider.hoverBackground': base === 'vs-dark' ? '#a3a3a3a6' : '#666666a6',
          'scrollbarSlider.activeBackground': base === 'vs-dark' ? '#a3a3a3cc' : '#666666cc',
          'scrollbar.shadow': '#00000000',
        },
      });
    }
  }, [dark]);
  const theme = dark ? 'remote-dark' : 'remote-light';
  return (
    <>
      {active.pendingMove && (
        <div className="editor-status">
          <ErrorText message={active.locationError} />
          <button onClick={() => onRetry(active.key)}>Retry move</button>
        </div>
      )}
      <div className="monaco-surface">
        <Editor
          loading={<EditorSkeleton />}
          theme={theme}
          path={fileUri(active)}
          value={active.text}
          options={{ ...options, readOnly: uploading || !!active.transferring }}
          onMount={setEditor}
          onChange={(value) => onChange(active.key, value ?? '')}
        />
      </div>
      <div className="editor-status">
        <span>UTF-8</span>
        <span>
          {active.transferring ? (
            'Opening in new window…'
          ) : active.pendingMove ? (
            'Move conflict · Changes kept'
          ) : !active.context ? (
            active.locationError ? (
              <button title={active.locationError} onClick={() => onRetry(active.key)}>
                Reconnect file
              </button>
            ) : (
              'Connecting file…'
            )
          ) : uploading ? (
            'Upload in progress · Read only'
          ) : active.saving ? (
            'Saving…'
          ) : active.text === active.original ? (
            'Saved'
          ) : (
            'Unsaved changes'
          )}
        </span>
      </div>
      {active.conflict && (
        <Modal
          title="This file changed remotely"
          description="Compare the remote version with your edits before saving."
          onClose={() => onDismissConflict(active.key)}
          wide
        >
          <div className="conflict-labels">
            <span>Remote version</span>
            <span>Your edits</span>
          </div>
          <div className="conflict-editor">
            <DiffEditor
              loading={<EditorSkeleton />}
              theme={theme}
              original={active.conflict.text}
              modified={active.text}
              options={{ ...options, readOnly: true, renderSideBySide: true, renderOverviewRuler: false }}
            />
          </div>
          <div className="actions">
            <button onClick={() => onDiscardConflict(active.key)}>Discard my edits</button>
            <button
              className="primary"
              disabled={active.saving || !!active.pendingMove}
              onClick={() => onSave(active.key, true)}
            >
              Save my version
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
