import { useState } from 'react';
import { call, failure } from '../bridge/client';
import type { SessionSnapshot } from '../bridge/types';
import type { ChatState, ChatUpdate } from '../chat/state';
import { useVisibleRefresh } from './visible';

const requests = new Map<string, { usage: ChatState['status']['usage']; snapshot: Promise<SessionSnapshot> }>();
const attempted = new Map<string, number>();
export function useSessionUsage(chat: ChatState | undefined, update: ChatUpdate | undefined, enabled: boolean) {
  const [loading, setLoading] = useState(false);
  const refresh = async (force = false) => {
    if (!chat || chat.closed || !update) return;
    const id = chat.session.id;
    let request = requests.get(id);
    if (!request) {
      const recent = Math.max(attempted.get(id) ?? 0, (chat.status.limits_updated_at ?? 0) * 1000);
      if (!force && Date.now() - recent < 60000) return;
      attempted.set(id, Date.now());
      request = {
        usage: chat.status.usage,
        snapshot: call('session_status', { id }).finally(() => requests.delete(id)),
      };
      requests.set(id, request);
    }
    const { usage, snapshot: pending } = request;
    setLoading(true);
    try {
      const snapshot = await pending;
      update((current) => ({
        ...current,
        status: {
          ...current.status,
          provider: snapshot.status.provider,
          usage:
            current.status.usage !== usage ? current.status.usage : (snapshot.status.usage ?? current.status.usage),
          ...((snapshot.status.limits_updated_at ?? 0) >= (current.status.limits_updated_at ?? 0)
            ? {
                limits: snapshot.status.limits,
                limits_error: snapshot.status.limits_error,
                limits_updated_at: snapshot.status.limits_updated_at,
              }
            : {}),
        },
      }));
    } catch (error) {
      update((current) => ({ ...current, status: { ...current.status, limits_error: failure(error).message } }));
    } finally {
      setLoading(false);
    }
  };
  useVisibleRefresh(
    () => {
      void refresh();
    },
    enabled && Boolean(chat && !chat.closed),
  );
  return { loading, refresh };
}
