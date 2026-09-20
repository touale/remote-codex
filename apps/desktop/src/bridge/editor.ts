export interface EditorView {
  line: number;
  column: number;
  top: number;
  left: number;
}
interface DocumentIdentity {
  server: string;
  root: string;
  path: string;
}
export type MarkdownMode = 'edit' | 'preview' | 'split';
export interface PreviewView {
  page?: number;
  scale?: number | 'fit';
}
export type FileDocument = DocumentIdentity & (TextDocument | { kind: 'preview'; previewView?: PreviewView });
interface TextDocument {
  kind: 'text';
  mode?: MarkdownMode;
  text: string;
  original: string;
  revision: string;
  view?: EditorView;
}
interface ChangeDocument {
  server: string;
  root: string;
  path: string;
  diff: string;
}
export type ContentTarget =
  | { kind: 'file'; context: string; transfer: string; document: FileDocument }
  | { kind: 'diff'; document: ChangeDocument };
