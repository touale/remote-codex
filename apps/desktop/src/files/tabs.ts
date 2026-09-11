import { failure } from '../bridge/client';
import type { TextFile } from '../bridge/types';
import { remotePath } from './context';

interface FileIdentity {
  key: string;
  context: string;
  server: string;
  root: string;
  path: string;
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
