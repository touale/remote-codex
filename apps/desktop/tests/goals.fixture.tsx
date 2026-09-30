import { useEffect, useRef, useState } from 'react';
import { emptyTiming, type SessionAction } from '../src/bridge/session';
import type { SessionSnapshot } from '../src/bridge/types';
import { SessionChat } from '../src/chat/SessionChat';
import { initialChat } from '../src/chat/state';
import { useChats } from '../src/chat/useChats';
import { DraftStore } from '../src/chat/drafts/store';
import { submitDraft } from '../src/chat/drafts/submit';

const settings = {
  mode: 'agent' as const,
  model: 'fixture',
  effort: null,
  full_access: false,
  approval_policy: 'on-request',
  reviewer: 'user',
};
const noop = () => {};
const report = (error: unknown) => {
  throw error;
};

export function GoalsFixture() {
  const chats = useChats(report, async () => null);
  const [id, setId] = useState('goal-fixture');
  const [actions, setActions] = useState<SessionAction[]>([]);
  const reject = useRef(false);
  const open = (id: string) => {
    chats.store.set(
      id,
      initialChat(
        { id, title: 'Goal fixture', cwd: '/fixture', created_at: 0, updated_at: 0, archived: false, state: 'idle' },
        'fixture',
        settings,
        [],
      ),
    );
    return { id, workspace: { id: 'workspace', server: 'fixture', path: '/fixture' } };
  };
  useEffect(() => {
    open('goal-fixture');
    const original = window.fetch;
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol !== 'ipc:' || !['/session_action', '/session_snapshot'].includes(url.pathname))
        return original.call(window, input, init);
      const { id, action } = JSON.parse(init!.body as string) as { id: string; action?: SessionAction };
      if (!id.startsWith('goal-fixture')) return original.call(window, input, init);
      let response: unknown = null;
      if (action) {
        setActions((previous) => [...previous, action]);
        if (action.action === 'goal') {
          if (action.goal.action !== 'set' && action.goal.action !== 'resume')
            throw new Error(`Unexpected fixture goal action: ${action.goal.action}`);
          if (reject.current) {
            reject.current = false;
            return new Response(JSON.stringify({ code: 'GOAL_REJECTED', message: 'Fixture goal request failed.' }), {
              headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' },
            });
          }
          const current = chats.store.get(id)!;
          const goal =
            action.goal.action === 'set'
              ? {
                  thread_id: id,
                  objective: action.goal.objective,
                  token_budget: action.goal.token_budget,
                  status: 'active' as const,
                  tokens_used: 0,
                  time_used_seconds: 0,
                }
              : current.goal && {
                  ...current.goal,
                  status: 'active' as const,
                };
          if (!goal) throw new Error('Fixture goal is missing.');
          chats.store.receive(id, { type: 'goal_changed', goal });
          if (action.goal.action === 'set')
            chats.store.receive(id, { type: 'turn_started', id: 'goal-turn', timing: emptyTiming() });
        } else if (action.action === 'settings') {
          chats.store.receive(id, { type: 'settings_changed', settings: { ...settings, ...action.settings } });
        } else if (action.action === 'submit') {
          response = { turn_id: 'message-turn', sent_at: 1 };
        } else throw new Error(`Unexpected fixture action: ${action.action}`);
      } else {
        const chat = chats.store.get(id)!;
        response = {
          goal: chat.goal,
          plan: chat.plan,
          settings: chat.settings,
          status: chat.status,
          environment: chat.environment,
          turn: chat.turn,
          current_turn: chat.turn ? chat.turns[chat.turn] : null,
          pending: [],
          closed: false,
        } satisfies SessionSnapshot;
      }
      return new Response(JSON.stringify(response), {
        headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'ok' },
      });
    };
    return () => {
      window.fetch = original;
    };
  }, []);
  const finish = () => {
    const chat = chats.store.get(id)!;
    if (chat.goal) chats.store.receive(id, { type: 'goal_changed', goal: { ...chat.goal, status: 'complete' } });
    if (chat.turn)
      chats.store.receive(id, {
        type: 'turn_completed',
        id: chat.turn,
        timing: emptyTiming(),
        outcome: { status: 'completed' },
      });
  };
  const fresh = async () => {
    const drafts = new DraftStore();
    const key = drafts.ensure({ server: 'fixture', path: '/fixture' });
    drafts.update(key, (draft) => ({ ...draft, mode: 'goal', text: 'A fresh goal.' }));
    await submitDraft(
      drafts,
      key,
      { action: 'goal', goal: { action: 'set', objective: 'A fresh goal.', token_budget: null } },
      async () => open('goal-fixture-draft'),
      chats,
      (opened) => setId(opened.id),
    );
  };
  return (
    <>
      <div>
        <button
          onClick={() => {
            reject.current = true;
          }}
        >
          Reject next goal request
        </button>
        <button
          onClick={() => {
            const chat = chats.store.get(id)!;
            if (chat.goal) chats.store.receive(id, { type: 'goal_changed', goal: { ...chat.goal, status: 'paused' } });
            chats.store.receive(id, {
              type: 'environment_changed',
              state: { status: 'reconnecting', attempt: 1, max_attempts: 10, retry_in_ms: 5000 },
            });
          }}
        >
          Disconnect goal
        </button>
        <button onClick={() => chats.store.receive(id, { type: 'environment_changed', state: { status: 'ready' } })}>
          Restore goal connection
        </button>
        <button onClick={finish}>Complete goal</button>
        <button onClick={() => void fresh().catch(report)}>Start draft goal</button>
      </div>
      <output id="goal-actions">{JSON.stringify(actions)}</output>
      <SessionChat
        id={id}
        controller={chats}
        onResume={noop}
        onFreshPlan={async () => {}}
        onDiff={noop}
        report={report}
      />
    </>
  );
}
