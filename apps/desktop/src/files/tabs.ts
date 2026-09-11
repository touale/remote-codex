import { failure } from '../bridge/client';
import type { TextFile } from '../bridge/types';
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
  original: string;
  saving: boolean;
  conflict?: TextFile;
}
export type FileTab =
  | (FileIdentity & { status: 'loading' })
  | (FileIdentity & { status: 'failed'; error: string })
  | (Buffer & { status: 'ready' });
const isReady = (tab: FileTab): tab is Buffer & { status: 'ready' } => tab.status === 'ready';
const protectedTab = (tab: FileTab) =>
  Boolean(tab.pendingMove || (isReady(tab) && (tab.saving || tab.text !== tab.original)));
export const fileKey = (server: string, root: string, path: string) => JSON.stringify([server, remotePath(root, path)]);

// Owns read lifetimes independently of tabs: closing a tab invalidates its result,
// but its native file context stays retained until the accepted read settles.
export class FileTabs {
  private tabs: FileTab[] = [];
  private listeners = new Set<() => void>();
  private requests = new Map<symbol, { context: string; task: Promise<void> }>();
  private generations = new Map<string, symbol>();
  constructor(private read: (context: string, path: string) => Promise<TextFile>) {}

  snapshot = () => this.tabs;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(tabs = this.tabs) {
    this.tabs = [...tabs];
    this.listeners.forEach((listener) => listener());
  }
  get = (key: string) => this.tabs.find((tab) => tab.key === key);
  buffers = () => this.tabs.filter(isReady);
  update = (key: string, values: Partial<Buffer>) => {
    this.publish(this.tabs.map((tab) => (tab.key === key && isReady(tab) ? { ...tab, ...values } : tab)));
  };
  close = (key: string) => {
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
      this.generations.delete(tab.key);
      const relative = relativePath(tab.root, next);
      const root = relative === undefined ? '/' : tab.root;
      const path = relative ?? next.slice(1);
      const key = fileKey(server, root, path);
      const target = tabs.find((other) => other !== tab && other.key === key);
      const waiting =
        tab.status === 'ready' ? { saving: false } : { status: 'failed' as const, error: 'Opening the moved file…' };
      if (target && protectedTab(target)) {
        tabs = tabs.map((current) =>
          current === tab
            ? {
                ...tab,
                ...waiting,
                pendingMove: next,
                locationError: `This file moved to ${next}. Close the conflicting destination tab, then retry.`,
              }
            : current,
        );
        continue;
      }
      if (target) {
        this.generations.delete(target.key);
        tabs = tabs.filter((current) => current !== target);
      }
      const file = {
        ...tab,
        ...waiting,
        root,
        path,
        context: relative === undefined ? '' : tab.context,
        key,
        pendingMove: undefined,
        locationError: undefined,
        ...(isReady(tab) ? { conflict: tab.conflict && { ...tab.conflict, path } } : {}),
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
    return tab?.status === 'failed' ? this.load(tab) : Promise.resolve();
  };
  private load({ key, context, server, root, path }: FileIdentity) {
    const token = Symbol(key);
    const file = { key, context, server, root, path };
    this.generations.set(key, token);
    const replace = (tab: FileTab) => this.publish([...this.tabs.filter((t) => t.key !== key), tab]);
    // Preserve tab order when retrying an existing file.
    const settle = (tab: FileTab) => {
      if (this.generations.get(key) === token)
        this.publish(this.tabs.map((current) => (current.key === key ? tab : current)));
    };
    const task = Promise.resolve()
      .then(() => this.read(context, path))
      .then(
        (content) => settle({ ...content, ...file, status: 'ready', original: content.text, saving: false }),
        (error) => settle({ ...file, status: 'failed', error: failure(error).message }),
      )
      .finally(() => {
        this.requests.delete(token);
        if (this.generations.get(key) === token) this.generations.delete(key);
        this.publish();
      });
    this.requests.set(token, { context, task });
    if (this.get(key)) settle({ ...file, status: 'loading' });
    else replace({ ...file, status: 'loading' });
    return task;
  }
}
