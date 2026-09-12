import type { editor } from 'monaco-editor';
import type { EditorView } from '../bridge/editor';
const editors = new Map<string, editor.IStandaloneCodeEditor>();
export function captureView(key: string): EditorView | undefined {
  const editor = editors.get(key);
  const position = editor?.getPosition();
  return editor && position
    ? { line: position.lineNumber, column: position.column, top: editor.getScrollTop(), left: editor.getScrollLeft() }
    : undefined;
}
export function registerEditor(key: string, editor: editor.IStandaloneCodeEditor, view?: EditorView) {
  editors.set(key, editor);
  if (view) {
    editor.setPosition({ lineNumber: view.line, column: view.column });
    editor.setScrollPosition({ scrollTop: view.top, scrollLeft: view.left });
  }
  return () => {
    if (editors.get(key) === editor) editors.delete(key);
  };
}
