import { useEffect, useState } from 'react';
import { SessionChat } from '../src/chat/SessionChat';
import { initialChat } from '../src/chat/state';
import { useChats } from '../src/chat/useChats';

const session = {
  id: 'history-fixture',
  title: 'History',
  cwd: '/fixture',
  created_at: 0,
  updated_at: 0,
  archived: false,
  state: 'idle',
};
const settings = {
  mode: 'agent' as const,
  model: 'fixture',
  effort: null,
  full_access: false,
  approval_policy: 'on-request',
  reviewer: 'user',
};
const report = (error: unknown) => {
  throw error;
};
const noop = () => {};

export function HistoryFixture() {
  const chats = useChats(report, async () => null);
  const [visible, setVisible] = useState(true);
  useEffect(() => {
    const chat = initialChat(session, 'fixture', settings, []);
    chat.historyReady = false;
    chats.store.set(session.id, chat);
    void chats.loadHistory(session.id).catch(report);
  }, []);
  return (
    <>
      <button onClick={() => setVisible((value) => !value)}>Toggle conversation</button>
      {visible && (
        <SessionChat
          id={session.id}
          controller={chats}
          onResume={noop}
          onFreshPlan={async () => {}}
          onDiff={noop}
          report={report}
        />
      )}
    </>
  );
}
