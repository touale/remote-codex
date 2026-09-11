import { failure } from '../../bridge/client';
import type { WorkspaceTarget } from '../../bridge/files';
import type { SessionAction } from '../../bridge/session';
import type { useChats } from '../useChats';
import type { DraftStore, OpenedDraft } from './store';

export type DraftSubmission = Extract<SessionAction, { action: 'submit' | 'goal' }>;
export async function submitDraft(
  store: DraftStore,
  key: string,
  action: DraftSubmission,
  open: (target: WorkspaceTarget) => Promise<OpenedDraft | null>,
  chats: Pick<ReturnType<typeof useChats>, 'store' | 'action' | 'update' | 'close'>,
  activate: (opened: OpenedDraft) => void,
) {
  const draft = store.get(key);
  if (!draft || draft.preparing) return;
  const submission = draft.pending ?? {
    action: structuredClone(action),
    text: action.action === 'submit' ? action.text : action.goal.action === 'set' ? action.goal.objective : draft.text,
    mode: draft.mode,
    settings: { ...draft.settings },
    sentAt: Math.floor(Date.now() / 1000),
  };
  store.update(key, (current) => ({
    ...current,
    pending: submission,
    text: current.pending ? current.text : '',
    preparing: true,
    phase: 'connecting',
    error: null,
  }));
  try {
    let opened = draft.opened;
    if (!opened || chats.store.get(opened.id)?.closed) opened = await open(draft.target);
    if (!opened) throw new Error('Session setup was cancelled. Your draft is saved.');
    if (!store.get(key)) {
      await chats.close(opened.id);
      return;
    }
    const bound = opened;
    store.update(key, (current) => ({ ...current, opened: bound, phase: 'settings' }));
    await chats.action(bound.id, {
      action: 'settings',
      settings: { ...submission.settings, mode: submission.mode === 'plan' ? 'plan' : 'agent' },
    });
    chats.update(bound.id, (chat) => ({ ...chat, draft: '', composerMode: submission.mode }));
    store.update(key, (current) => ({ ...current, phase: 'submitting' }));
    try {
      await chats.action(bound.id, structuredClone(submission.action));
    } catch (error) {
      if (failure(error).outcome_unknown) {
        chats.update(bound.id, (chat) => ({
          ...chat,
          draft: store.get(key)?.text ?? '',
          warning: failure(error).message,
        }));
        activate(bound);
        store.remove(key);
      }
      throw error;
    }
    const latest = store.get(key);
    chats.update(bound.id, (chat) => ({ ...chat, draft: latest?.text ?? '' }));
    activate(bound);
    store.remove(key);
  } catch (error) {
    store.update(key, (current) => ({ ...current, error: failure(error).message }));
    throw error;
  } finally {
    store.update(key, (current) => ({ ...current, preparing: false }));
  }
}
