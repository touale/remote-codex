import { expect, it } from 'vitest';
import { initialChat } from '../state';
import { preparePlanDraft } from './plan';
import { DraftStore } from './store';

it('preserves unsent work and reopens an unfinished plan draft without resubmitting', () => {
  const store = new DraftStore();
  const target = { server: 'dev', path: '/workspace/test' };
  const ordinary = store.ensure(target);
  store.update(ordinary, (draft) => ({ ...draft, text: 'Unsent workspace work' }));
  const chat = initialChat(
    { id: 'source', title: '', cwd: target.path, created_at: 0, updated_at: 0, archived: false, state: 'idle' },
    target.server,
    {
      mode: 'plan',
      model: 'selected-model',
      effort: 'high',
      full_access: false,
      approval_policy: 'on-request',
      reviewer: 'user',
    },
    [],
  );
  chat.draft = 'Unsent conversation work';
  const plan = { id: 'plan', role: 'assistant' as const, text: 'The revised plan.', plan: true, complete: true };
  const { key, action } = preparePlanDraft(store, chat, plan);
  if (!action) throw new Error('A new plan should be submitted.');
  expect(key).not.toBe(ordinary);
  store.update(key, (draft) => ({
    ...draft,
    pending: { action, text: action.text, settings: draft.settings, mode: 'code', sentAt: 1 },
    text: 'Next instruction',
    error: 'Connection interrupted',
  }));
  const pending = store.get(key)?.pending;
  expect(preparePlanDraft(store, chat, plan)).toEqual({ key, action: null });
  expect(store.get(key)).toMatchObject({ pending, text: 'Next instruction', error: 'Connection interrupted' });
  expect(store.get(ordinary)?.text).toBe('Unsent workspace work');
  expect(chat.draft).toBe('Unsent conversation work');
});

it.each(['on-request', 'never'])('does not broaden %s approvals when carrying a plan into a new session', (policy) => {
  const store = new DraftStore();
  const chat = initialChat(
    { id: 'source', title: '', cwd: '/workspace/test', created_at: 0, updated_at: 0, archived: false, state: 'idle' },
    'dev',
    {
      mode: 'plan',
      model: 'selected-model',
      effort: 'high',
      full_access: true,
      approval_policy: policy,
      reviewer: 'user',
    },
    [],
  );
  const { key } = preparePlanDraft(store, chat, {
    id: 'plan',
    role: 'assistant',
    text: 'A plan.',
    plan: true,
    complete: true,
  });
  expect(store.get(key)?.settings.permissions).toBe(policy === 'never' ? 'full_access' : 'workspace');
});
