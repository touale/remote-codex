import { failure } from '../bridge/client';
import type { TextFile } from '../bridge/types';
import type { FileDocument, EditorView, MarkdownMode, PreviewView } from '../bridge/editor';
import type { PreviewContent } from './preview';
import { movedPath, relativePath, remotePath, within } from './context';

interface FileIdentity {
  key: string;
  context: string;
  server: string;
  root: string;
  path: string;
  locationError?: string;
  pendingMove?: string;
}
export interface Buffer extends FileIdentity, TextFile {
  transferring?: string;
  view?: EditorView;
  mode?: MarkdownMode;
  original: string;
  saving: boolean;
  conflict?: TextFile;
}
export interface PreviewBuffer extends FileIdentity, PreviewContent {
  transferring?: string;
  previewView?: PreviewView;
}
export type ReadyFile = (Buffer | PreviewBuffer) & { status: 'ready' };
export type FileTab =
  (FileIdentity & { status: 'loading' }) | (FileIdentity & { status: 'failed'; error: string }) | ReadyFile;
export const isText = (tab: FileTab): tab is Buffer & { status: 'ready' } =>
  tab.status === 'ready' && !('preview' in tab);
const protectedTab = (tab: FileTab) =>
  Boolean(
    tab.pendingMove ||
    (tab.status === 'ready' && tab.transferring) ||
    (isText(tab) && (tab.saving || tab.text !== tab.original)),
  );
export const fileKey = (server: string, root: string, path: string) => JSON.stringify([server, remotePath(root, path)]);

// Owns read lifetimes independently of tabs: closing a tab invalidates its result,
// but its native file context stays retained until the accepted read settles.
export class FileTabs {
  private tabs: FileTab[] = [];
  private listeners = new Set<() => void>();
  private requests = new Map<symbol, { context: string; task: Promise<void>; controller: AbortController }>();
  private generations = new Map<string, symbol>();
  constructor(
    private read: (context: string, path: string, signal: AbortSignal) => Promise<TextFile | PreviewContent>,
  ) {}

  snapshot = () => this.tabs;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(tabs = [...this.tabs]) {
    this.tabs = tabs;
    this.listeners.forEach((listener) => listener());
  }
  get = (key: string) => this.tabs.find((tab) => tab.key === key);
  buffer = (key: string) => {
    const tab = this.get(key);
    return tab && isText(tab) ? tab : undefined;
  };
  buffers = () => this.tabs.filter(isText);
  update = (key: string, values: Partial<Buffer>) => {
    const current = this.get(key);
    if (!current || !isText(current) || (current.transferring && values.text !== undefined)) return;
    this.publish(this.tabs.map((tab) => (tab.key === key && isText(tab) ? { ...tab, ...values } : tab)));
  };
  ready = (key: string) => {
    const tab = this.get(key);
    return tab?.status === 'ready' ? tab : undefined;
  };
  setTransfer = (key: string, transferring?: string) => {
    this.publish(this.tabs.map((tab) => (tab.key === key && tab.status === 'ready' ? { ...tab, transferring } : tab)));
  };
  setPreviewView = (key: string, previewView: PreviewView) => {
    this.publish(this.tabs.map((tab) => (tab.key === key && 'preview' in tab ? { ...tab, previewView } : tab)));
  };
  close = (key: string) => {
    this.requests.get(this.generations.get(key)!)?.controller.abort();
    this.generations.delete(key);
    this.publish(this.tabs.filter((tab) => tab.key !== key));
  };
  bind = (key: string, context: string) => {
    this.publish(
      this.tabs.map((tab) =>
        tab.key === key && !tab.pendingMove ? { ...tab, context, locationError: undefined } : tab,
      ),
    );
  };
  locationFailed = (key: string, error: string) => {
    this.publish(this.tabs.map((tab) => (tab.key === key ? { ...tab, locationError: error } : tab)));
  };
  assertWritable = (key: string) => {
    const tab = this.get(key);
    if (tab?.pendingMove) throw new Error(tab.locationError);
    if (tab?.status === 'ready' && tab.transferring) throw new Error('This file is moving to a new window.');
  };
  adopt = (context: string, file: FileDocument & { kind: 'text' }) => {
    const key = fileKey(file.server, file.root, file.path);
    this.publish([
      ...this.tabs.filter((t) => t.key !== key),
      { ...file, key, context, status: 'ready', saving: false },
    ]);
    return key;
  };
  assertCanMove = (server: string, source: string, destination: string) => {
    if (
      this.tabs.some((tab) => {
        const path = tab.pendingMove ?? remotePath(tab.root, tab.path);
        return tab.server === server && !within(source, path) && within(destination, path) && protectedTab(tab);
      })
    )
      throw new Error('Resolve unsaved changes in the destination tab before moving this file.');
  };
  relocate = (server: string, source: string, destination: string) => {
    const keys = new Map<string, string>();
    let tabs = [...this.tabs];
    // Move actual file bindings first, then the drafts waiting for those bindings.
    for (const tab of [...this.tabs].sort((a, b) => Number(!!a.pendingMove) - Number(!!b.pendingMove))) {
      const absolute = tab.pendingMove ?? remotePath(tab.root, tab.path);
      const next = movedPath(absolute, source, destination);
      if (
        tab.server !== server ||
        !within(source, absolute) ||
        (!tab.pendingMove && next === absolute) ||
        !tabs.includes(tab)
      )
        continue;
      this.requests.get(this.generations.get(tab.key)!)?.controller.abort();
      this.generations.delete(tab.key);
      const relative = relativePath(tab.root, next);
      const root = relative === undefined ? '/' : tab.root;
      const path = relative ?? next.slice(1);
      const key = fileKey(server, root, path);
      const target = tabs.find((other) => other !== tab && other.key === key);
      const moved: FileTab = isText(tab)
        ? { ...tab, saving: false }
        : tab.status === 'ready'
          ? tab
          : { ...tab, status: 'failed', error: 'Opening the moved file…' };
      if (target && protectedTab(target)) {
        tabs = tabs.map((current) =>
          current === tab
            ? {
                ...moved,
                pendingMove: next,
                locationError: `This file moved to ${next}. Close the conflicting destination tab, then retry.`,
              }
            : current,
        );
        continue;
      }
      if (target) {
        this.requests.get(this.generations.get(target.key)!)?.controller.abort();
        this.generations.delete(target.key);
        tabs = tabs.filter((current) => current !== target);
      }
      const file = {
        ...moved,
        root,
        path,
        context: relative === undefined ? '' : tab.context,
        key,
        pendingMove: undefined,
        locationError: undefined,
        ...(isText(tab) ? { conflict: tab.conflict && { ...tab.conflict, path } } : {}),
      };
      keys.set(tab.key, file.key);
      tabs = tabs.map((current) => (current === tab ? file : current));
    }
    this.publish(tabs);
    return keys;
  };
  retainedContexts = () =>
    new Set([...this.tabs.map((tab) => tab.context), ...[...this.requests.values()].map((request) => request.context)]);
  open = (file: FileIdentity): Promise<void> => {
    const existing = this.get(file.key);
    if (existing) return this.requests.get(this.generations.get(file.key)!)?.task ?? Promise.resolve();
    return this.load(file);
  };
  retry = (key: string): Promise<void> => {
    const tab = this.get(key);
    return tab && (tab.status === 'failed' || 'preview' in tab) ? this.load(tab) : Promise.resolve();
  };
  private load({ key, context, server, root, path }: FileIdentity) {
    const token = Symbol(key);
    const controller = new AbortController();
    const file = { key, context, server, root, path };
    this.generations.set(key, token);
    const settle = (tab: FileTab) => {
      this.requests.delete(token);
      if (this.generations.get(key) === token) {
        this.generations.delete(key);
        this.publish(this.tabs.map((current) => (current.key === key ? tab : current)));
      } else {
        // A closed or moved tab may still retain its context until this read settles.
        this.publish();
      }
    };
    const task = Promise.resolve()
      .then(() => this.read(context, path, controller.signal))
      .then(
        (content) =>
          settle(
            'preview' in content
              ? { ...content, ...file, status: 'ready' }
              : { ...content, ...file, status: 'ready', original: content.text, saving: false },
          ),
        (error) => settle({ ...file, status: 'failed', error: failure(error).message }),
      );
    this.requests.set(token, { context, task, controller });
    const loading: FileTab = { ...file, status: 'loading' };
    // Preserve tab order when retrying an existing file.
    this.publish(
      this.get(key) ? this.tabs.map((current) => (current.key === key ? loading : current)) : [...this.tabs, loading],
    );
    return task;
  }
}
