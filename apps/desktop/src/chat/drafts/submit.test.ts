import { expect, it, vi } from 'vitest';
import { initialChat } from '../state';
import { ChatStore } from '../store';
import { DraftStore } from './store';
import { submitDraft } from './submit';

function fixture() {
  const drafts = new DraftStore();
  const target = { server: 'dev', path: '/workspace/test' };
  const key = drafts.ensure(target);
  drafts.update(key, (draft) => ({
    ...draft,
    text: 'Build this.',
    mode: 'plan',
    settings: { model: 'selected-model' },
  }));
  const store = new ChatStore();
  store.set(
    'native-id',
    initialChat(
      { id: 'native-id', title: '', cwd: target.path, created_at: 0, updated_at: 0, archived: false, state: 'idle' },
      'dev',
      {
        mode: 'agent',
        model: 'default-model',
        effort: null,
        full_access: false,
        approval_policy: 'on-request',
        reviewer: 'user',
      },
      [],
    ),
  );
  const chats = {
    store,
    update: store.update,
    action: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
  };
  const opened = { id: 'native-id', workspace: { ...target, id: 'workspace-id' } };
  return { drafts, key, chats, opened };
}
it('keeps failed drafts and reuses the created session, sending once after settings are acknowledged', async () => {
  const { drafts, key, chats, opened } = fixture();
  const open = vi.fn().mockResolvedValue(opened);
  const activate = vi.fn();
  chats.action.mockRejectedValueOnce(new Error('Settings rejected'));
  await expect(
    submitDraft(drafts, key, { action: 'submit', text: 'Build this.' }, open, chats, activate),
  ).rejects.toThrow('Settings rejected');
  expect(drafts.get(key)?.text).toBe('');
  expect(drafts.get(key)?.pending?.text).toBe('Build this.');
  expect(activate).not.toHaveBeenCalled();
  const calls: string[] = [];
  chats.action.mockImplementation(async (_, action) => {
    calls.push(action.action);
    if (action.action === 'settings') {
      expect(action.settings).toMatchObject({ mode: 'plan', model: 'selected-model' });
      drafts.update(key, (draft) => ({ ...draft, text: 'Next instruction.' }));
      await submitDraft(drafts, key, { action: 'submit', text: 'Duplicate' }, open, chats, activate);
    }
  });
  await submitDraft(drafts, key, { action: 'submit', text: 'Build this.' }, open, chats, activate);
  expect(open).toHaveBeenCalledTimes(1);
  expect(calls).toEqual(['settings', 'submit']);
  expect(chats.store.get(opened.id)?.draft).toBe('Next instruction.');
  expect(drafts.get(key)).toBeUndefined();
  expect(activate).toHaveBeenCalledOnce();
});
it('hands an uncertain submission to the real session instead of retaining a retryable new draft', async () => {
  const { drafts, key, chats, opened } = fixture();
  const activate = vi.fn();
  chats.action
    .mockResolvedValueOnce(undefined)
    .mockRejectedValueOnce({ message: 'Inspect the session before retrying.', outcome_unknown: true });
  await expect(
    submitDraft(
      drafts,
      key,
      { action: 'goal', goal: { action: 'set', objective: 'Build this.', token_budget: null } },
      async () => opened,
      chats,
      activate,
    ),
  ).rejects.toMatchObject({ outcome_unknown: true });
  expect(drafts.get(key)).toBeUndefined();
  expect(activate).toHaveBeenCalledWith(opened);
  expect(chats.store.get(opened.id)?.warning).toContain('Inspect');
});

it('preserves the draft when setup is cancelled before a native session exists', async () => {
  const { drafts, key, chats } = fixture();
  const activate = vi.fn();
  await expect(
    submitDraft(drafts, key, { action: 'submit', text: 'Build this.' }, async () => null, chats, activate),
  ).rejects.toThrow('cancelled');
  expect(drafts.get(key)?.text).toBe('');
  expect(drafts.get(key)?.pending?.text).toBe('Build this.');
  expect(drafts.get(key)?.opened).toBeNull();
  expect(chats.action).not.toHaveBeenCalled();
  expect(activate).not.toHaveBeenCalled();
});

it('separates the submitted snapshot from an identical next draft while setup is pending', async () => {
  const { drafts, key, chats, opened } = fixture();
  let ready!: (value: typeof opened) => void;
  const open = vi.fn(
    () =>
      new Promise<typeof opened>((resolve) => {
        ready = resolve;
      }),
  );
  const action = { action: 'submit' as const, text: 'Build this.' };
  const operation = submitDraft(drafts, key, action, open, chats, vi.fn());
  expect(drafts.get(key)?.text).toBe('');
  expect(drafts.get(key)?.pending?.text).toBe('Build this.');
  action.text = 'Changed caller object';
  drafts.update(key, (draft) => ({ ...draft, text: 'Build this.', settings: { model: 'late-model' } }));
  ready(opened);
  await operation;
  expect(chats.action.mock.calls[0][1]).toMatchObject({ settings: { model: 'selected-model' } });
  expect(chats.action.mock.calls[1][1]).toEqual({ action: 'submit', text: 'Build this.' });
  expect(chats.store.get(opened.id)?.draft).toBe('Build this.');
});
