import Editor, { DiffEditor, loader } from '@monaco-editor/react';
import 'monaco-editor/esm/vs/basic-languages/css/css.contribution';
import 'monaco-editor/esm/vs/basic-languages/html/html.contribution';
import 'monaco-editor/esm/vs/basic-languages/ini/ini.contribution';
import 'monaco-editor/esm/vs/basic-languages/javascript/javascript.contribution';
import 'monaco-editor/esm/vs/basic-languages/markdown/markdown.contribution';
import 'monaco-editor/esm/vs/basic-languages/python/python.contribution';
import 'monaco-editor/esm/vs/basic-languages/rust/rust.contribution';
import 'monaco-editor/esm/vs/basic-languages/shell/shell.contribution';
import 'monaco-editor/esm/vs/basic-languages/sql/sql.contribution';
import 'monaco-editor/esm/vs/basic-languages/typescript/typescript.contribution';
import 'monaco-editor/esm/vs/basic-languages/yaml/yaml.contribution';
import 'monaco-editor/esm/vs/editor/editor.all';
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api';
import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import { useEffect } from 'react';
import { useTransferLock } from '../transfers/store';
import { Modal } from '../ui/controls';
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
  onChange,
  onDismissConflict,
  onDiscardConflict,
}: {
  active: Buffer;
  dark: boolean;
  onSave: (key: string, resolve?: boolean) => void;
  onChange: (key: string, text: string) => void;
  onDismissConflict: (key: string) => void;
  onDiscardConflict: (key: string) => void;
}) {
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
      <div className="monaco-surface">
        <Editor
          loading={<EditorSkeleton />}
          theme={theme}
          path={fileUri(active)}
          value={active.text}
          options={{ ...options, readOnly: uploading }}
          onChange={(value) => onChange(active.key, value ?? '')}
        />
      </div>
      <div className="editor-status">
        <span>UTF-8</span>
        <span>
          {uploading
            ? 'Upload in progress · Read only'
            : active.saving
              ? 'Saving…'
              : active.text === active.original
                ? 'Saved'
                : 'Unsaved changes'}
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
            <button className="primary" disabled={active.saving} onClick={() => onSave(active.key, true)}>
              Save my version
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
