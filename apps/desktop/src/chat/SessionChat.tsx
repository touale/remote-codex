import { useCallback, useSyncExternalStore } from 'react';
import type { SessionAction } from '../bridge/session';
import type { FileChange } from '../files/useFiles';
import { Chat } from './Chat';
import type { ChatState, ChatUpdate, Message } from './state';
import type { useChats } from './useChats';

export function SessionChat({
  id,
  controller,
  onResume,
  onFreshPlan,
  onDiff,
  report,
}: {
  id: string | null;
  controller: Pick<ReturnType<typeof useChats>, 'store' | 'update' | 'action' | 'loadHistory'>;
  onResume: () => void;
  onFreshPlan: (chat: ChatState, plan: Message) => Promise<void>;
  onDiff: (change: FileChange) => void;
  report: (error: unknown) => void;
}) {
  const { store, update, action, loadHistory } = controller;
  const subscribe = useCallback((listener: () => void) => (id ? store.subscribe(id, listener) : () => {}), [store, id]);
  const snapshot = useCallback(() => (id ? store.get(id) : undefined), [store, id]);
  const chat = useSyncExternalStore(subscribe, snapshot);
  const change = useCallback<ChatUpdate>(
    (value) => {
      if (id) update(id, (previous) => (typeof value === 'function' ? value(previous) : { ...previous, ...value }));
    },
    [id, update],
  );
  const execute = useCallback(
    async (value: SessionAction) => {
      if (id) await action(id, value);
    },
    [id, action],
  );
  const history = useCallback(() => {
    if (id) void loadHistory(id, store.get(id)?.nextCursor).catch(report);
  }, [id, store, loadHistory, report]);
  return (
    <Chat
      key={id ?? 'empty'}
      chat={chat}
      update={change}
      onAction={execute}
      onResume={onResume}
      onFreshPlan={onFreshPlan}
      onHistory={history}
      onDiff={onDiff}
      report={report}
    />
  );
}
