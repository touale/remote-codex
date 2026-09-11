import type { HistoryPage } from '../bridge/types';
import type { ChatState, Message } from './state';

function historyMessages(page: HistoryPage): Message[] {
  return [...page.turns]
    .sort((a, b) => a.id.localeCompare(b.id))
    .flatMap((turn) => {
      let firstUser = true;
      return turn.items.map((item): Message => {
        const user = item.kind === 'userMessage';
        const sentAt = item.sent_at ?? (user && firstUser ? turn.timing.started_at : null);
        if (user) firstUser = false;
        return {
          id: item.id,
          clientId: item.client_id ?? undefined,
          turn: turn.id,
          role: user ? 'user' : 'assistant',
          sentAt: sentAt ?? undefined,
          phase: item.phase ?? undefined,
          text: item.text,
          tool: item.tool,
          plan: item.kind === 'plan',
          complete: turn.status !== 'inProgress',
        };
      });
    })
    .filter((item) => item.text || item.tool);
}
export function mergeHistory(chat: ChatState, page: HistoryPage): ChatState {
  const items = historyMessages(page);
  const live = new Map(
    chat.messages.flatMap((message) => [
      [message.id, message] as const,
      ...(message.clientId ? [[message.clientId, message] as const] : []),
    ]),
  );
  const history = items.map((item) => {
    const current = live.get(item.id) ?? (item.clientId ? live.get(item.clientId) : undefined);
    if (!current) return item;
    const source =
      item.complete || item.text.length > current.text.length ? { ...current, ...item } : { ...item, ...current };
    return { ...source, id: item.id, sentAt: current.sentAt ?? item.sentAt };
  });
  const ids = new Set(items.flatMap((item) => [item.id, item.clientId]));
  return {
    ...chat,
    session: page.session,
    nextCursor: page.next_cursor,
    turns: {
      ...chat.turns,
      ...Object.fromEntries(
        page.turns.map(({ id, status, timing }) => [
          id,
          status === 'inProgress' && chat.turns[id]?.status !== 'inProgress' && chat.turns[id]
            ? chat.turns[id]
            : { id, status, timing },
        ]),
      ),
    },
    messages: [...history, ...chat.messages.filter((m) => !ids.has(m.id) && !(m.clientId && ids.has(m.clientId)))],
  };
}
