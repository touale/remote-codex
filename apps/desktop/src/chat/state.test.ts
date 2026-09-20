import { describe, expect, it } from 'vitest';
import { emptyTiming } from '../bridge/session';
import type { HistoryPage } from '../bridge/types';
import { mergeHistory, replaceHistory } from './history';
import { initialChat, reduceEvent } from './state';
const session = {
  id: 'thread',
  title: '',
  cwd: '/project',
  created_at: 0,
  updated_at: 0,
  archived: false,
  state: 'idle',
};
const settings = {
  mode: 'agent' as const,
  model: 'model-a',
  effort: 'medium',
  full_access: false,
  approval_policy: 'on-request',
  reviewer: 'user',
};
describe('native session projection', () => {
  it('replaces reverted turns without resurrecting late messages and retains async choices', () => {
    let state = initialChat(session, 'dev', settings, []);
    state.draft = 'Keep the composer draft';
    state = reduceEvent(state, {
      type: 'message',
      turn_id: 'removed',
      item_id: 'old',
      text: 'Old answer',
      phase: null,
      complete: true,
    });
    const questions = [{ title: 'Which scope?', options: ['Research', 'Implementation'] }];
    state = replaceHistory(
      state,
      {
        session,
        next_cursor: 'earlier',
        turns: [
          {
            id: 'kept',
            status: 'completed',
            timing: emptyTiming(),
            items: [
              {
                id: 'question',
                client_id: null,
                phase: null,
                kind: 'agentMessage',
                text: '',
                sent_at: null,
                delivery: 'async',
                questions,
              },
            ],
          },
        ],
      },
      ['removed'],
    );
    state = reduceEvent(state, {
      type: 'message',
      turn_id: 'removed',
      item_id: 'late',
      text: 'Late answer',
      phase: null,
      complete: true,
    });
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0]).toMatchObject({ delivery: 'async', questions });
    expect(state.draft).toBe('Keep the composer draft');
    expect(state.nextCursor).toBe('earlier');
    state = reduceEvent(state, { type: 'session_updated', session: { ...session, title: 'Updated title' } });
    expect(state.session.title).toBe('Updated title');
  });
  it('replaces a streamed message with its acknowledged complete content', () => {
    let state = initialChat(session, 'dev', settings, []);
    state = reduceEvent(state, {
      type: 'message',
      turn_id: 'turn',
      phase: null,
      item_id: 'item',
      text: 'Hel',
      complete: false,
    });
    state = reduceEvent(state, {
      type: 'message',
      turn_id: 'turn',
      phase: null,
      item_id: 'item',
      text: 'lo',
      complete: false,
    });
    state = reduceEvent(state, {
      type: 'message',
      turn_id: 'turn',
      phase: null,
      item_id: 'item',
      text: 'Hello!',
      complete: true,
    });
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].text).toBe('Hello!');
  });
  it('keeps approved settings and task state across a transient disconnect', () => {
    let state = initialChat(session, 'dev', settings, []);
    state = reduceEvent(state, { type: 'turn_started', id: 'turn', timing: emptyTiming() });
    state = reduceEvent(state, { type: 'settings_changed', settings: { ...settings, model: 'model-b' } });
    state = reduceEvent(state, {
      type: 'environment_changed',
      state: { status: 'reconnecting', attempt: 1, max_attempts: 10, retry_in_ms: 5000 },
    });
    expect(state.turn).toBe('turn');
    expect(state.settings.model).toBe('model-b');
    expect(state.closed).toBe(false);
    state = reduceEvent(state, { type: 'closed', reason: 'Taken over' });
    expect(state.turn).toBeNull();
    expect(state.closed).toBe(true);
  });
  it('keeps separate pending approvals and removes only the resolved request', () => {
    let state = initialChat(session, 'dev', settings, []);
    for (const id of ['first', 'second'])
      state = reduceEvent(state, { type: 'approval_requested', request_id: id, description: id });
    state = reduceEvent(state, { type: 'interaction_resolved', request_id: 'first' });
    expect(state.questions.map((q) => q.id)).toEqual(['second']);
  });
});

it('deduplicates a live follow-up against native history without losing its actual send time', () => {
  let state = initialChat(session, 'dev', settings, []);
  state.messages = [{ id: 'client', clientId: 'client', role: 'user', text: 'Follow up', sentAt: 125 }];
  state = reduceEvent(state, {
    type: 'user_message',
    item_id: 'native',
    client_id: 'client',
    turn_id: 'turn',
    text: 'Follow up',
  });
  const page: HistoryPage = {
    session,
    next_cursor: null,
    turns: [
      {
        id: 'turn',
        status: 'completed',
        timing: { started_at: 100, completed_at: 130, duration_ms: 29500 },
        items: [
          { id: 'first', client_id: null, phase: null, kind: 'userMessage', text: 'Start', sent_at: null },
          { id: 'native', client_id: 'client', phase: null, kind: 'userMessage', text: 'Follow up', sent_at: 125 },
          {
            id: 'old',
            client_id: null,
            phase: null,
            kind: 'userMessage',
            text: 'Unknown older follow-up',
            sent_at: null,
          },
        ],
      },
    ],
  };
  state = mergeHistory(state, page);
  expect(state.messages.map((m) => [m.id, m.sentAt])).toEqual([
    ['first', 100],
    ['native', 125],
    ['old', undefined],
  ]);
  expect(state.turns.turn.timing.duration_ms).toBe(29500);
  const reopened = mergeHistory(initialChat(session, 'dev', settings, []), page);
  expect(reopened.messages).toEqual(state.messages);
});
it('retains metadata and the new active turn when an older completion arrives', () => {
  let state = initialChat(session, 'dev', settings, []);
  state = reduceEvent(state, { type: 'turn_started', id: 'old', timing: { ...emptyTiming(), started_at: 100 } });
  state = reduceEvent(state, { type: 'turn_started', id: 'new', timing: { ...emptyTiming(), started_at: 200 } });
  state = reduceEvent(state, {
    type: 'turn_completed',
    id: 'old',
    timing: emptyTiming(),
    outcome: { status: 'interrupted' },
  });
  expect(state.turn).toBe('new');
  expect(state.turns.old.timing.started_at).toBe(100);
  expect(state.turns.old.timing.completed_at).toBeNull();
});

it('preserves known usage across empty snapshots and ignores restoration overtaken by live usage', async () => {
  const { applySnapshot } = await import('./state');
  const state = initialChat(session, 'dev', settings, []);
  const snapshot = {
    goal: null,
    plan: null,
    settings,
    status: state.status,
    environment: state.environment,
    turn: null,
    current_turn: null,
    pending: [],
    closed: false,
  };
  const recorded = {
    last_tokens: 1000,
    total_tokens: 8000,
    input_tokens: 900,
    cached_input_tokens: 100,
    output_tokens: 100,
    reasoning_tokens: 50,
    context_window: 100000,
  };
  const known = reduceEvent(state, { type: 'usage_changed', usage: recorded });
  expect(applySnapshot(known, snapshot).status.usage).toEqual(recorded);
  const compacted = { ...recorded, last_tokens: 100, total_tokens: 8200 };
  const live = reduceEvent(known, { type: 'usage_changed', usage: compacted });
  const older = { ...snapshot, status: { ...snapshot.status, usage: recorded } };
  expect(applySnapshot(live, older, recorded).status.usage).toEqual(compacted);
  expect(applySnapshot(state, older).status.usage).toEqual(recorded);
  const reverted = { ...state, discardedTurns: ['removed'] };
  const stale = { ...older, current_turn: { id: 'removed', status: 'completed', timing: emptyTiming() } };
  expect(applySnapshot(reverted, stale).turns).toEqual({});
  expect(applySnapshot(reverted, stale).status.usage).toBeNull();
});
