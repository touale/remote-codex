import {
  emptyStatus,
  emptyTiming,
  restoredMode,
  type ComposerMode,
  type SessionStatus,
  type TurnState,
} from '../bridge/session';
import { mergeTool } from './toolState';
import type {
  AsyncQuestion,
  Environment,
  Goal,
  Interaction,
  Model,
  Plan,
  Session,
  SessionEvent,
  SessionSnapshot,
  Settings,
  ToolItem,
} from '../bridge/types';
export interface Message {
  id: string;
  clientId?: string;
  role: 'user' | 'assistant';
  delivery?: string;
  questions?: AsyncQuestion[];
  turn?: string;
  text: string;
  sentAt?: number;
  phase?: string;
  tool?: ToolItem;
  plan?: boolean;
  complete?: boolean;
}
export type ChatUpdate = (change: Partial<ChatState> | ((chat: ChatState) => ChatState)) => void;
export interface Question {
  id: string;
  description?: string;
  interaction?: Interaction;
}
export interface MessageEdit {
  id: string;
  clientId: string;
  turn: string;
  text: string;
  removedTurns: string[];
  reverted?: boolean;
  uncertain?: boolean;
  busy?: boolean;
  error?: string;
}
export interface ChatState {
  goal: Goal | null;
  plan: Plan | null;
  session: Session;
  server: string;
  settings: Settings;
  models: Model[];
  messages: Message[];
  questions: Question[];
  turn: string | null;
  turns: Record<string, TurnState>;
  status: SessionStatus;
  composerMode: ComposerMode;
  environment: Environment;
  draft: string;
  edit?: MessageEdit;
  scroll: number | null;
  closed: boolean;
  warning: string | null;
  nextCursor: string | null;
  // Native ownership can be established before the first history read succeeds.
  historyReady: boolean;
  discardedTurns: string[];
}
export function initialChat(session: Session, server: string, settings: Settings, models: Model[]): ChatState {
  return {
    goal: null,
    plan: null,
    session,
    server,
    settings,
    models,
    messages: [],
    questions: [],
    turn: null,
    turns: {},
    status: emptyStatus(),
    composerMode: restoredMode(settings, null),
    environment: { status: 'ready' },
    draft: '',
    scroll: null,
    closed: false,
    warning: null,
    nextCursor: null,
    historyReady: true,
    discardedTurns: [],
  };
}
export function applySnapshot(
  chat: ChatState,
  snapshot: SessionSnapshot,
  usageAtRequest = chat.status.usage,
): ChatState {
  const discarded = !!snapshot.current_turn && chat.discardedTurns.includes(snapshot.current_turn.id);
  const current = discarded ? null : snapshot.current_turn;
  return snapshot.pending.reduce(reduceEvent, {
    ...chat,
    goal: snapshot.goal,
    plan: snapshot.plan && !chat.discardedTurns.includes(snapshot.plan.turn) ? snapshot.plan : null,
    settings: snapshot.settings,
    environment: snapshot.environment,
    status: {
      ...snapshot.status,
      usage: discarded
        ? null
        : chat.status.usage !== usageAtRequest
          ? chat.status.usage
          : (snapshot.status.usage ?? chat.status.usage),
    },
    turn: snapshot.turn && !chat.discardedTurns.includes(snapshot.turn) ? snapshot.turn : null,
    turns: current ? { ...chat.turns, [current.id]: current } : chat.turns,
    closed: snapshot.closed,
    questions: [],
  });
}
export function reduceEvent(state: ChatState, event: SessionEvent): ChatState {
  const turnId =
    'turn_id' in event
      ? event.turn_id
      : event.type === 'plan_changed'
        ? event.plan.turn
        : event.type === 'turn_started' || event.type === 'turn_completed'
          ? event.id
          : undefined;
  if (turnId && state.discardedTurns.includes(turnId)) return state;
  switch (event.type) {
    case 'session_updated':
      return { ...state, session: event.session };
    case 'activity_changed':
      return { ...state, status: { ...state.status, activity: event.activity, active_flags: event.active_flags } };
    case 'usage_changed':
      return { ...state, status: { ...state.status, usage: event.usage } };
    case 'rate_limits_changed':
      return {
        ...state,
        status: { ...state.status, limits: event.limits, limits_error: null, limits_updated_at: Date.now() / 1000 },
      };
    case 'goal_changed':
      return {
        ...state,
        goal: event.goal,
        composerMode: event.goal?.status === 'active' ? 'goal' : state.composerMode,
      };
    case 'plan_changed':
      return { ...state, plan: event.plan };
    case 'user_message': {
      const previous = state.messages.find(
        (m) => m.id === event.item_id || (event.client_id && m.clientId === event.client_id),
      );
      return {
        ...state,
        messages: upsert(state.messages, {
          ...previous,
          id: event.item_id,
          clientId: event.client_id ?? undefined,
          turn: event.turn_id,
          role: 'user',
          text: event.text,
          sentAt:
            previous?.sentAt ??
            (state.messages.some((m) => m.turn === event.turn_id && m.role === 'user')
              ? undefined
              : (state.turns[event.turn_id]?.timing.started_at ?? undefined)),
        }),
      };
    }
    case 'plan_message':
    case 'message': {
      const previous = state.messages.find((m) => m.id === event.item_id);
      return {
        ...state,
        messages: upsert(state.messages, {
          ...previous,
          id: event.item_id,
          role: 'assistant',
          turn: event.turn_id || previous?.turn || state.turn || undefined,
          phase: event.type === 'message' ? (event.phase ?? previous?.phase) : undefined,
          plan: event.type === 'plan_message',
          delivery: event.type === 'message' ? (event.delivery ?? previous?.delivery) : undefined,
          questions: event.type === 'message' && event.complete ? event.questions : previous?.questions,
          complete: event.complete,
          text: event.complete ? event.text : (previous?.text ?? '') + event.text,
        }),
      };
    }
    case 'tool_changed':
      return {
        ...state,
        messages: upsert(state.messages, {
          id: event.item.id,
          role: 'assistant',
          text: '',
          tool: mergeTool(state.messages.find((m) => m.id === event.item.id)?.tool, event.item),
          turn: event.turn_id || state.turn || undefined,
        }),
      };
    case 'tool_output': {
      const previous = state.messages.find((m) => m.id === event.item_id);
      const tool = previous?.tool ?? {
        id: event.item_id,
        kind: 'commandExecution',
        title: 'Command',
        status: 'inProgress',
        output: '',
        changes: [],
      };
      return {
        ...state,
        messages: upsert(state.messages, {
          ...previous,
          id: event.item_id,
          role: 'assistant',
          text: '',
          turn: event.turn_id || state.turn || undefined,
          tool: { ...tool, output: (tool.output + event.text).slice(-262144) },
        }),
      };
    }
    case 'approval_requested':
      return {
        ...state,
        questions: uniqueQuestion(state.questions, { id: event.request_id, description: event.description }),
      };
    case 'interaction_requested':
      return {
        ...state,
        questions: uniqueQuestion(state.questions, { id: event.request_id, interaction: event.interaction }),
      };
    case 'interaction_required':
      return { ...state, warning: `Unsupported interaction: ${event.kind}` };
    case 'interaction_resolved':
      return { ...state, questions: state.questions.filter((q) => q.id !== event.request_id) };
    case 'turn_started':
      return {
        ...state,
        turn: event.id,
        warning: null,
        plan: null,
        turns: { ...state.turns, [event.id]: { id: event.id, status: 'inProgress', timing: event.timing } },
      };
    case 'turn_completed': {
      const previous = state.turns[event.id]?.timing ?? emptyTiming();
      return {
        ...state,
        turn: state.turn === event.id ? null : state.turn,
        turns: {
          ...state.turns,
          [event.id]: {
            id: event.id,
            status: event.outcome.status,
            timing: { ...event.timing, started_at: event.timing.started_at ?? previous.started_at },
          },
        },
        warning: event.outcome.status === 'failed' ? (event.outcome.message ?? 'Task failed.') : state.warning,
      };
    }
    case 'settings_changed':
      return {
        ...state,
        settings: event.settings,
        composerMode:
          event.settings.mode === 'plan' ? 'plan' : state.composerMode === 'plan' ? 'code' : state.composerMode,
      };
    case 'permissions_updated':
      return { ...state, settings: { ...state.settings, full_access: event.full_access } };
    case 'environment_changed':
      return { ...state, environment: event.state };
    case 'warning':
      return { ...state, warning: event.message };
    case 'closed':
      return {
        ...state,
        closed: true,
        turn: null,
        environment: { status: 'closed' },
        questions: [],
        warning: event.reason,
      };
  }
}
function upsert(messages: Message[], message: Message): Message[] {
  const matches = (m: Message) => m.id === message.id || Boolean(message.clientId && m.clientId === message.clientId);
  return messages.some(matches) ? messages.map((m) => (matches(m) ? { ...m, ...message } : m)) : [...messages, message];
}
function uniqueQuestion(questions: Question[], question: Question): Question[] {
  return [...questions.filter((q) => q.id !== question.id), question];
}
