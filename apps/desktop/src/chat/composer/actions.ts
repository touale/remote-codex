import type { ComposerMode, SessionAction } from '../../bridge/session';
import type { ComposerState } from './model';

/** Every mode entry point shares the same acknowledged transition. */
export async function changeMode(
  chat: Pick<ComposerState, 'turn' | 'goal' | 'settings'>,
  mode: ComposerMode,
  onAction: (action: SessionAction) => Promise<void>,
) {
  if (chat.turn) throw new Error('Stop the current task before changing mode.');
  if (mode !== 'goal' && chat.goal?.status === 'active') await onAction({ action: 'goal', goal: { action: 'pause' } });
  const native = mode === 'plan' ? 'plan' : 'agent';
  if (chat.settings.mode !== native) await onAction({ action: 'settings', settings: { mode: native } });
}
