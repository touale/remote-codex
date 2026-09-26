import { useEffect, useState } from 'react';
import { flushSync } from 'react-dom';
import { SessionChat } from '../src/chat/SessionChat';
import { initialChat, type ChatState } from '../src/chat/state';
import { emptyTiming } from '../src/bridge/session';
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
    let beforeStream: ChatState | undefined;
    const stream = () =>
      flushSync(() => {
        if (!beforeStream) {
          beforeStream = chats.store.get(session.id);
          chats.store.receive(session.id, { type: 'turn_started', id: 'zz-stream', timing: emptyTiming() });
          chats.store.receive(session.id, {
            type: 'tool_changed',
            turn_id: 'zz-stream',
            item: {
              id: 'stream-tool',
              kind: 'commandExecution',
              title: 'Streaming command',
              status: 'inProgress',
              output: '',
              changes: [],
            },
          });
        }
        chats.store.receive(session.id, {
          type: 'message',
          turn_id: 'zz-stream',
          item_id: 'stream-text',
          text: 'A new paragraph of streamed output.\n\n',
          phase: null,
          complete: false,
        });
        chats.store.receive(session.id, {
          type: 'tool_output',
          turn_id: 'zz-stream',
          item_id: 'stream-tool',
          text: 'Log entry\n',
        });
        chats.store.flush();
      });
    const reset = () =>
      flushSync(() => {
        if (beforeStream) chats.store.set(session.id, beforeStream);
        beforeStream = undefined;
      });
    window.addEventListener('fixture-stream-chunk', stream);
    window.addEventListener('fixture-stream-reset', reset);
    return () => {
      window.removeEventListener('fixture-stream-chunk', stream);
      window.removeEventListener('fixture-stream-reset', reset);
    };
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
