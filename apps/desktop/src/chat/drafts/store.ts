import type { WorkspaceTarget } from '../../bridge/files';

import type { ComposerMode } from '../../bridge/session';
import type { RequestedSettings } from '../composer/model';
import type { DraftSubmission } from './submit';

export interface OpenedDraft {
  workspace: WorkspaceTarget & { id: string };
  id: string;
}
interface PendingSubmission {
  action: DraftSubmission;
  text: string;
  mode: ComposerMode;
  settings: RequestedSettings;
  sentAt: number;
}
export interface Draft {
  target: WorkspaceTarget;
  text: string;
  focus: number;
  mode: ComposerMode;
  settings: RequestedSettings;
  pending: PendingSubmission | null;
  preparing: boolean;
  phase: 'connecting' | 'settings' | 'submitting';
  error: string | null;
  opened: OpenedDraft | null;
}
const workspaceKey = (target: WorkspaceTarget) => JSON.stringify([target.server, target.path]);
export class DraftStore {
  private values = new Map<string, Draft>();
  private listeners = new Map<string, Set<() => void>>();
  get = (key: string) => this.values.get(key);
  ensure(target: WorkspaceTarget) {
    const key = workspaceKey(target);
    if (!this.values.has(key))
      this.values.set(key, {
        target,
        text: '',
        focus: 0,
        mode: 'code',
        settings: {},
        pending: null,
        preparing: false,
        phase: 'connecting',
        error: null,
        opened: null,
      });
    this.update(key, (draft) => ({ ...draft, focus: draft.focus + 1 }));
    return key;
  }
  update(key: string, change: (draft: Draft) => Draft) {
    const current = this.get(key);
    if (!current) return;
    this.values.set(key, change(current));
    this.emit(key);
  }
  subscribe(key: string, listener: () => void) {
    const listeners = this.listeners.get(key) ?? new Set();
    listeners.add(listener);
    this.listeners.set(key, listeners);
    return () => {
      listeners.delete(listener);
      if (!listeners.size) this.listeners.delete(key);
    };
  }
  remove(key: string) {
    this.values.delete(key);
    this.emit(key);
  }
  forget(server: string, path?: string) {
    for (const [key, draft] of this.values) {
      if (draft.target.server === server && (path === undefined || draft.target.path === path)) this.remove(key);
    }
  }
  private emit(key: string) {
    for (const listener of this.listeners.get(key) ?? []) listener();
  }
}
