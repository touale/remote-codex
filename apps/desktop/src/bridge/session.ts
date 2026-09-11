import type { Goal, Settings } from './types';

export interface Submission {
  turn_id: string;
  sent_at: number | null;
}
export interface TurnTiming {
  started_at: number | null;
  completed_at: number | null;
  duration_ms: number | null;
}
export interface TurnState {
  id: string;
  status: string;
  timing: TurnTiming;
}
export interface TokenUsage {
  last_tokens: number;
  total_tokens: number;
  input_tokens: number;
  cached_input_tokens: number;
  output_tokens: number;
  reasoning_tokens: number;
  context_window: number | null;
}
export interface RateWindow {
  used_percent: number;
  window_minutes: number | null;
  resets_at: number | null;
}
export interface RateLimit {
  id: string | null;
  is_default: boolean;
  name: string;
  primary: RateWindow | null;
  secondary: RateWindow | null;
}
export interface SessionStatus {
  provider: string | null;
  activity: string;
  active_flags: string[];
  usage: TokenUsage | null;
  limits: RateLimit[];
  limits_error: string | null;
  limits_updated_at: number | null;
}
type GoalAction =
  | { action: 'set'; objective: string; token_budget: number | null }
  | { action: 'budget'; token_budget: number | null }
  | { action: 'pause' | 'resume' | 'clear' };
export type SessionAction =
  | { action: 'submit'; text: string; client_id?: string }
  | { action: 'steer'; text: string; turn: string; client_id?: string }
  | { action: 'interrupt'; turn: string }
  | {
      action: 'settings';
      settings: {
        mode?: Settings['mode'];
        model?: string;
        effort?: string;
        permissions?: 'workspace' | 'full_access';
        reviewer?: 'user' | 'auto_review';
      };
    }
  | { action: 'goal'; goal: GoalAction }
  | { action: 'approve'; request: string; decision: 'accept_once' | 'cancel' }
  | {
      action: 'interact';
      request: string;
      answer: {
        action: 'accept' | 'decline' | 'cancel';
        values: Record<string, string>;
      };
    }
  | { action: 'retry' };
export type ComposerMode = 'code' | 'plan' | 'goal';
export function restoredMode(settings: Settings, goal: Goal | null): ComposerMode {
  return settings.mode === 'plan' ? 'plan' : goal?.status === 'active' ? 'goal' : 'code';
}
export function emptyStatus(): SessionStatus {
  return {
    provider: null,
    activity: 'notLoaded',
    active_flags: [],
    usage: null,
    limits: [],
    limits_error: null,
    limits_updated_at: null,
  };
}
export const emptyTiming = (): TurnTiming => ({ started_at: null, completed_at: null, duration_ms: null });

export interface AccountUsage {
  account_id: string | null;
  availability: 'available' | 'signed_out' | 'unsupported';
  limits: RateLimit[];
}
