import type { CachedSession } from '../bridge/types';
import type { ChatSummary } from './store';
import { activityTimestamp } from './activity';

export function mergeSessions(cached: CachedSession[], chats: Record<string, ChatSummary>): CachedSession[] {
  const sessions = new Map(cached.map((item) => [item.session.id, item]));
  for (const chat of Object.values(chats)) {
    const previous = sessions.get(chat.session.id);
    if (chat.closed && previous) continue;
    sessions.set(chat.session.id, {
      server_id: previous?.server_id ?? '',
      server: chat.server,
      session: {
        ...chat.session,
        updated_at: Math.max(
          activityTimestamp(chat.session.updated_at),
          activityTimestamp(previous?.session.updated_at ?? 0),
        ),
      },
    });
  }
  return [...sessions.values()];
}
