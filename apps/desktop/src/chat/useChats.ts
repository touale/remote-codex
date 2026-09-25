import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { call, failure, listen } from '../bridge/client';
import { restoredMode, type SessionAction } from '../bridge/session';
import type { SessionEvent, SessionOpened } from '../bridge/types';
import type { Ask } from '../ui/useDialog';
import { mergeHistory, replaceHistory } from './history';
import { applySnapshot, initialChat, reduceEvent } from './state';
import { ChatStore } from './store';

export function useChats(report: (error: unknown) => void, ask: Ask) {
  const [store] = useState(() => new ChatStore());
  const chats = useSyncExternalStore(store.subscribeSummaries, store.getSummaries);
  const pending = useRef(new Map<string, SessionEvent[]>());
  const update = store.update;
  const historyEpoch = useRef(new Map<string, number>());
  const historyRequests = useRef(new Map<string, Promise<void>>());
  const loadHistory = useCallback(
    (id: string, cursor: string | null = null, active: () => boolean = () => true) => {
      let epoch = historyEpoch.current.get(id) ?? 0;
      const existing = historyRequests.current.get(JSON.stringify([id, cursor, epoch]));
      if (existing) return existing;
      // A refreshed tail supersedes an older in-flight page and its cursor.
      if (cursor === null) historyEpoch.current.set(id, ++epoch);
      const key = JSON.stringify([id, cursor, epoch]);
      update(id, (chat) => ({ ...chat, historyLoading: true, historyError: null }));
      const task = call('session_history', { id, cursor })
        .then((page) => {
          if (active() && (historyEpoch.current.get(id) ?? 0) === epoch)
            update(id, (chat) => ({
              ...mergeHistory(chat, page, cursor !== null),
              historyReady: cursor === null || chat.historyReady,
            }));
        })
        .catch((error) => {
          if (!active()) return;
          if (failure(error).code === 'OPERATION_CANCELLED') throw error;
          if ((historyEpoch.current.get(id) ?? 0) === epoch)
            update(id, (chat) => ({ ...chat, historyError: { message: failure(error).message, cursor } }));
        })
        .finally(() => {
          historyRequests.current.delete(key);
          if ((historyEpoch.current.get(id) ?? 0) === epoch) update(id, (chat) => ({ ...chat, historyLoading: false }));
        });
      historyRequests.current.set(key, task);
      return task;
    },
    [update],
  );
  useEffect(
    () =>
      listen((event) => {
        if (event.kind === 'session') {
          if (!store.get(event.id)) {
            const queue = pending.current.get(event.id) ?? [];
            pending.current.set(event.id, [...queue.slice(-255), event.event]);
          } else store.receive(event.id, event.event);
        }
        if (event.kind === 'resync')
          return (async () => {
            await loadHistory(event.id);
            const usage = store.get(event.id)?.status.usage;
            const snapshot = await call('session_snapshot', { id: event.id });
            update(event.id, (chat) => applySnapshot(chat, snapshot, usage));
          })().catch(report);
      }),
    [loadHistory, report, update, store],
  );
  const register = useCallback(
    (opened: Extract<SessionOpened, { status: 'open' }>, server: string, resumed = false) => {
      const id = opened.session.id;
      const previous = store.get(id);
      if (previous && !previous.closed)
        return store.set(id, { ...previous, settings: opened.settings, models: opened.models });
      let chat = applySnapshot(initialChat(opened.session, server, opened.settings, opened.models), opened.snapshot);
      chat.historyReady = !resumed;
      chat.composerMode = restoredMode(opened.settings, opened.snapshot.goal);
      if (previous)
        chat = {
          ...chat,
          status: { ...chat.status, usage: chat.status.usage ?? previous.status.usage },
          messages: previous.messages,
          turns: { ...previous.turns, ...chat.turns },
          draft: previous.draft,
          edit: previous.edit && {
            ...previous.edit,
            busy: false,
            uncertain: previous.edit.uncertain || previous.edit.busy,
          },
          discardedTurns: previous.discardedTurns,
          scroll: previous.scroll,
        };
      for (const event of [...opened.snapshot.pending, ...(pending.current.get(id) ?? [])])
        chat = reduceEvent(chat, event);
      pending.current.delete(id);
      store.set(id, chat);
    },
    [store],
  );
  const action = useCallback(
    async (id: string, action: SessionAction) => {
      const message = action.action === 'submit' || action.action === 'steer' ? action : null;
      const clientId = message ? (message.client_id ?? crypto.randomUUID()) : null;
      if (message && clientId) {
        update(id, (chat) => ({
          ...chat,
          messages: [
            ...chat.messages,
            {
              id: clientId,
              clientId,
              role: 'user',
              text: message.text,
              sentAt: Math.floor(Date.now() / 1000),
              turn: message.action === 'steer' ? message.turn : undefined,
            },
          ],
        }));
      }
      try {
        const wireAction =
          action.action === 'submit' || action.action === 'steer' ? { ...action, client_id: clientId! } : action;
        const receipt = await call('session_action', { id, action: wireAction });
        if (message && clientId)
          update(id, (chat) => ({
            ...chat,
            draft: chat.edit?.clientId !== clientId && chat.draft.trim() === message.text ? '' : chat.draft,
            messages: chat.messages.map((item) =>
              item.clientId === clientId
                ? { ...item, turn: receipt?.turn_id ?? item.turn, sentAt: receipt?.sent_at ?? item.sentAt }
                : item,
            ),
          }));
      } catch (error) {
        if (clientId)
          update(id, (chat) => ({ ...chat, messages: chat.messages.filter((item) => item.id !== clientId) }));
        throw error;
      }
      if (action.action === 'settings' || action.action === 'goal') {
        try {
          const usage = store.get(id)?.status.usage;
          const snapshot = await call('session_snapshot', { id });
          update(id, (chat) => applySnapshot(chat, snapshot, usage));
        } catch (error) {
          if (action.action === 'goal' && action.goal.action === 'set')
            throw {
              ...failure(error),
              outcome_unknown: true,
              message:
                'The goal was submitted, but its state could not be read. Inspect this session before trying again.',
            };
          throw error;
        }
      }
      if (action.action === 'approve' || action.action === 'interact')
        update(id, (chat) => ({ ...chat, questions: chat.questions.filter((q) => q.id !== action.request) }));
    },
    [update],
  );
  const revert = useCallback(
    async (id: string, turn: string) => {
      historyEpoch.current.set(id, (historyEpoch.current.get(id) ?? 0) + 1);
      const result = await call('session_revert', { id, beforeTurnId: turn });
      update(id, (chat) => ({
        ...applySnapshot(replaceHistory(chat, result.history, chat.edit?.removedTurns), result.snapshot),
        edit: chat.edit && { ...chat.edit, reverted: true, uncertain: false },
      }));
    },
    [update],
  );
  const reloadEdit = useCallback(
    async (id: string) => {
      historyEpoch.current.set(id, (historyEpoch.current.get(id) ?? 0) + 1);
      const page = await call('session_history', { id, cursor: null });
      const snapshot = await call('session_snapshot', { id });
      update(id, (chat) => {
        const submitted = page.turns.some((t) => t.items.some((i) => i.client_id === chat.edit?.clientId));
        const reverted = !page.turns.some((t) => chat.edit?.removedTurns.includes(t.id));
        return {
          ...applySnapshot(replaceHistory(chat, page, reverted ? chat.edit?.removedTurns : []), snapshot),
          edit: !submitted && chat.edit ? { ...chat.edit, reverted, uncertain: false } : undefined,
        };
      });
    },
    [update],
  );
  const close = useCallback(
    async (id: string) => {
      await call('session_close', { id });
      update(id, (chat) => ({ ...chat, closed: true, turn: null, questions: [] }));
    },
    [update],
  );
  const metadata = async (id: string, action: 'rename' | 'archive' | 'close', savedTitle = '') => {
    if (action === 'close') {
      await close(id);
      return;
    }
    let name: string | null = null;
    if (action === 'rename') {
      name = await ask({
        title: 'Rename session',
        input: {
          label: 'Name',
          value: store.get(id)?.session.title ?? savedTitle,
        },
        choices: ['Save'],
      });
      if (!name) return;
    }
    if (store.get(id) && !store.get(id)?.closed) {
      if (
        !(await ask({
          title: 'Close this session first?',
          message: 'Changing its catalog entry requires releasing active session control.',
          choices: ['Close and continue'],
        }))
      )
        return;
      await close(id);
    }
    await call('session_metadata', { id, name, archived: action === 'archive' ? true : null });
    if (store.get(id))
      update(id, (chat) => ({
        ...chat,
        session: {
          ...chat.session,
          ...(name ? { title: name } : {}),
          ...(action === 'archive' ? { archived: true } : {}),
        },
      }));
  };
  useEffect(() => () => store.dispose(), [store]);
  return { chats, store, register, update, action, close, loadHistory, metadata, revert, reloadEdit };
}
