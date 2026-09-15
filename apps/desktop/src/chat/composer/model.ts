import type { ComposerMode, SessionAction } from '../../bridge/session';
import type { Goal, Model } from '../../bridge/types';

/** Presentation state shared by a local draft and an acknowledged native session. */
export interface ComposerState {
  server: string;
  draft: string;
  composerMode: ComposerMode;
  goal: Goal | null;
  turn: string | null;
  ready: boolean;
  canReconnect?: boolean;
  models: Model[];
  settingsState?: 'loading' | 'error' | 'ready';
  settings: {
    mode: 'agent' | 'plan';
    model: string | null;
    effort: string | null;
    full_access: boolean | null;
    reviewer: string | null;
  };
}
export type RequestedSettings = Extract<SessionAction, { action: 'settings' }>['settings'];
