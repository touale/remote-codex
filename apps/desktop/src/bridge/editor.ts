export interface EditorView {
  line: number;
  column: number;
  top: number;
  left: number;
}
export interface FileDocument {
  server: string;
  root: string;
  path: string;
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
