import type { SessionEvent } from '../bridge/types';
import { reduceEvent, type ChatState } from './state';

export type ChatSummary = Pick<ChatState, 'session' | 'server' | 'closed' | 'turn' | 'environment' | 'questions'>;
type Listener = () => void;

/** Session content has its own subscription; the app shell observes only navigation metadata. */
export class ChatStore {
  private states = new Map<string, ChatState>();
  private summaries: Record<string, ChatSummary> = {};
  private listeners = new Map<string, Set<Listener>>();
  private overview = new Set<Listener>();
  private pending = new Set<string>();
  private frame: number | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;

  get = (id: string): ChatState | undefined => this.states.get(id);
  getSummaries = () => this.summaries;
  subscribe = (id: string, listener: Listener) => {
    const listeners = this.listeners.get(id) ?? new Set<Listener>();
    listeners.add(listener);
    this.listeners.set(id, listeners);
    return () => {
      listeners.delete(listener);
      if (!listeners.size) this.listeners.delete(id);
    };
  };
  subscribeSummaries = (listener: Listener) => {
    this.overview.add(listener);
    return () => {
      this.overview.delete(listener);
    };
  };
  set(id: string, next: ChatState, deferred = false) {
    const previous = this.states.get(id);
    if (previous === next) return;
    this.states.set(id, next);
    if (
      !previous ||
      previous.session !== next.session ||
      previous.server !== next.server ||
      previous.closed !== next.closed ||
      previous.turn !== next.turn ||
      previous.environment !== next.environment ||
      previous.questions !== next.questions
    ) {
      this.summaries = {
        ...this.summaries,
        [id]: {
          session: next.session,
          server: next.server,
          closed: next.closed,
          turn: next.turn,
          environment: next.environment,
          questions: next.questions,
        },
      };
      for (const listener of this.overview) listener();
    }
    if (!deferred) {
      this.pending.delete(id);
      this.emit(id);
    } else {
      this.pending.add(id);
      if (this.frame === null) {
        this.frame = requestAnimationFrame(this.flush);
        // Hidden windows still ingest/acknowledge events and keep their snapshots current.
        this.timer = setTimeout(this.flush, 50);
      }
    }
  }
  update = (id: string, change: (chat: ChatState) => ChatState) => {
    const previous = this.get(id);
    if (previous) this.set(id, change(previous));
  };
  receive(id: string, event: SessionEvent) {
    const previous = this.get(id);
    if (!previous) return;
    const deferred =
      event.type === 'tool_output' ||
      event.type === 'tool_changed' ||
      event.type === 'usage_changed' ||
      ((event.type === 'message' || event.type === 'plan_message') && !event.complete);
    this.set(id, reduceEvent(previous, event), deferred);
  }
  flush = () => {
    this.cancelFrame();
    const pending = [...this.pending];
    this.pending.clear();
    for (const id of pending) this.emit(id);
  };
  dispose() {
    this.cancelFrame();
    this.pending.clear();
  }
  private emit(id: string) {
    for (const listener of this.listeners.get(id) ?? []) listener();
  }
  private cancelFrame() {
    if (this.frame !== null) cancelAnimationFrame(this.frame);
    if (this.timer !== null) clearTimeout(this.timer);
    this.frame = null;
    this.timer = null;
  }
}
