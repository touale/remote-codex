import type { ChatState, Message } from '../state';
import { permissionPreset } from '../permissions';
import type { DraftStore } from './store';

/** A fresh context carries the approved plan and settings, never the conversation history. */
export function preparePlanDraft(store: DraftStore, chat: ChatState, plan: Message) {
  const target = { server: chat.server, path: chat.session.cwd };
  // Reopen an unfinished handoff without resubmitting or overwriting its next draft.
  const key = JSON.stringify([target.server, target.path, chat.session.id, plan.id]);
  const existing = store.get(key);
  store.ensure(target, key);
  if (existing) return { key, action: null };
  const text = `Implement the following plan in a fresh context. Treat the plan as the source of user intent, re-read files as needed, and carry the work through implementation and verification.\n\n${plan.text}`;
  store.update(key, (draft) => ({
    ...draft,
    text,
    mode: 'code',
    settings: {
      mode: 'agent',
      ...(chat.settings.model ? { model: chat.settings.model } : {}),
      ...(chat.settings.effort ? { effort: chat.settings.effort } : {}),
      // A custom combination must not silently become approval-free Full Access.
      permissions: permissionPreset(chat.settings) ?? 'workspace',
      reviewer: chat.settings.reviewer === 'auto_review' ? 'auto_review' : 'user',
    },
  }));
  return { key, action: { action: 'submit' as const, text } };
}
