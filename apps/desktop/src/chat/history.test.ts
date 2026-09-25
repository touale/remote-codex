import { expect, it, vi } from 'vitest';
import { createElement } from 'react';
import { renderToString } from 'react-dom/server';
import { call } from '../bridge/client';
import type { HistoryPage } from '../bridge/types';
import { mergeHistory } from './history';
import { initialChat } from './state';
import { useChats } from './useChats';

vi.mock('../bridge/client', () => ({ call: vi.fn(), failure: (error: unknown) => error, listen: () => () => {} }));
// Exercise the loader's real store and promises without a browser subscription.
vi.mock('react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('react')>()),
  useSyncExternalStore: (_subscribe: unknown, snapshot: () => unknown) => snapshot(),
}));

const session = {
  id: 'thread',
  title: '',
  cwd: '/project',
  created_at: 0,
  updated_at: 0,
  archived: false,
  state: 'idle',
};
const page = (ids: string[], before = false): HistoryPage => ({
  session,
  next_cursor: before ? 'older' : null,
  turns: [
    {
      id: 'turn',
      status: 'completed',
      items_before: before,
      timing: { started_at: 100, completed_at: 200, duration_ms: 100000 },
      items: ids.map((id) => ({ id, kind: 'userMessage', text: id, client_id: null, sent_at: null, phase: null })),
    },
  ],
});

it('keeps failed history local to its session and discards older requests after a refreshed tail', async () => {
  let controller: ReturnType<typeof useChats> | undefined;
  function Harness() {
    controller = useChats(vi.fn(), async () => null);
    return null;
  }
  renderToString(createElement(Harness));
  const chats = controller!;
  const initial = initialChat(
    session,
    'dev',
    {
      mode: 'agent',
      model: 'fixture',
      effort: null,
      full_access: false,
      approval_policy: 'on-request',
      reviewer: 'user',
    },
    [],
  );
  initial.historyReady = false;
  initial.draft = 'Unsent draft';
  chats.store.set(session.id, initial);
  vi.mocked(call).mockRejectedValueOnce({ code: 'HISTORY_UNAVAILABLE', message: 'Read failed' });
  await chats.loadHistory(session.id);
  expect(chats.store.get(session.id)).toMatchObject({
    closed: false,
    draft: 'Unsent draft',
    historyReady: false,
    historyLoading: false,
    historyError: { message: 'Read failed', cursor: null },
  });
  let finishOld!: (value: HistoryPage) => void;
  vi.mocked(call).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishOld = resolve;
      }) as never,
  );
  const old = chats.loadHistory(session.id, 'older');
  vi.mocked(call).mockResolvedValueOnce(page(['new'], true) as never);
  await chats.loadHistory(session.id);
  finishOld(page(['stale']));
  await old;
  expect(chats.store.get(session.id)).toMatchObject({
    draft: 'Unsent draft',
    historyReady: true,
    historyLoading: false,
    historyError: null,
    nextCursor: 'older',
  });
  expect(chats.store.get(session.id)?.messages.map((m) => m.id)).toEqual(['new']);
  let active = true;
  vi.mocked(call).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishOld = resolve;
      }) as never,
  );
  const cancelled = chats.loadHistory(session.id, 'older', () => active);
  active = false;
  finishOld(page(['hidden']));
  await cancelled;
  expect(chats.store.get(session.id)?.messages.map((m) => m.id)).toEqual(['new']);
  expect(chats.store.get(session.id)?.historyLoading).toBe(false);
  chats.store.dispose();
});

it('merges older fragments and refreshed tails in native order without duplicates or invented timestamps', () => {
  let chat = initialChat(
    session,
    'dev',
    {
      mode: 'agent',
      model: 'fixture',
      effort: null,
      full_access: false,
      approval_policy: 'on-request',
      reviewer: 'user',
    },
    [],
  );
  chat = mergeHistory(chat, page(['b', 'c'], true));
  expect(chat.messages[0].sentAt).toBeUndefined();
  expect(chat.messages[0].canEdit).toBe(false);
  chat = mergeHistory(chat, page(['a', 'b']), true);
  expect(chat.messages.map((m) => m.id)).toEqual(['a', 'b', 'c']);
  expect(chat.messages[0].sentAt).toBe(100);
  expect(chat.messages[0].canEdit).toBe(true);
  chat.messages.push({ id: 'live', turn: 'turn', role: 'assistant', text: 'stream' });
  chat = mergeHistory(chat, page(['c', 'd'], true));
  expect(chat.messages.map((m) => m.id)).toEqual(['a', 'b', 'c', 'd', 'live']);
  chat = mergeHistory(chat, page(['a', 'b']), true);
  expect(chat.messages.map((m) => m.id)).toEqual(['a', 'b', 'c', 'd', 'live']);
});
