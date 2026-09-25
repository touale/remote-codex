import type { HistoryPage } from '../bridge/types';
import type { ChatState, Message } from './state';
import { mergeTool } from './toolState';

function historyMessages(page: HistoryPage): Message[] {
  return [...page.turns]
    .sort((a, b) => a.id.localeCompare(b.id))
    .flatMap((turn) => {
      let firstUser = true;
      return turn.items.map((item): Message => {
        const user = item.kind === 'userMessage';
        const canEdit = user ? firstUser && !turn.items_before : undefined;
        const sentAt = item.sent_at ?? (user && firstUser && !turn.items_before ? turn.timing.started_at : null);
        if (user) firstUser = false;
        return {
          id: item.id,
          clientId: item.client_id ?? undefined,
          turn: turn.id,
          role: user ? 'user' : 'assistant',
          sentAt: sentAt ?? undefined,
          phase: item.phase ?? undefined,
          text: item.text,
          delivery: item.delivery ?? undefined,
          questions: item.questions,
          tool: item.tool,
          plan: item.kind === 'plan',
          complete: turn.status !== 'inProgress',
          canEdit,
        };
      });
    })
    .filter((item) => item.text || item.tool || item.questions?.length);
}
export function mergeHistory(chat: ChatState, page: HistoryPage, older = false): ChatState {
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
    return {
      ...source,
      id: item.id,
      sentAt: current.sentAt ?? item.sentAt,
      tool: item.tool ? mergeTool(current.tool, item.tool) : source.tool,
    };
  });
  const replacements = new Map(
    history.flatMap((item) => [[item.id, item] as const, ...(item.clientId ? [[item.clientId, item] as const] : [])]),
  );
  const messages = chat.messages.map(
    (item) => replacements.get(item.id) ?? (item.clientId ? replacements.get(item.clientId) : undefined) ?? item,
  );
  // Native item IDs are not chronological. Keep existing order and use overlaps
  // as anchors, so a refreshed tail cannot move ahead of already loaded history.
  let previous: Message | undefined;
  for (const [index, item] of history.entries()) {
    if (!messages.some((message) => message.id === item.id)) {
      const next = history
        .slice(index + 1)
        .find((candidate) => candidate.turn === item.turn && messages.some((message) => message.id === candidate.id));
      let at = next ? messages.findIndex((message) => message.id === next.id) : -1;
      if (at < 0 && previous?.turn === item.turn) at = messages.findIndex((message) => message.id === previous!.id) + 1;
      if (at < 0) at = older ? 0 : messages.length;
      messages.splice(at, 0, item);
    }
    previous = item;
  }
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
    messages: messages.sort((a, b) => (a.turn && b.turn ? a.turn.localeCompare(b.turn) : a.turn ? -1 : b.turn ? 1 : 0)),
  };
}

/** A revert replaces history; normal pagination only merges it. */
export function replaceHistory(chat: ChatState, page: HistoryPage, removed: string[] = []): ChatState {
  return mergeHistory(
    {
      ...chat,
      messages: [],
      turns: {},
      turn: null,
      plan: null,
      questions: [],
      status: { ...chat.status, usage: null },
      warning: null,
      nextCursor: null,
      historyReady: true,
      historyError: null,
      historyLoading: false,
      discardedTurns: [...new Set([...chat.discardedTurns, ...removed])],
    },
    page,
  );
}
