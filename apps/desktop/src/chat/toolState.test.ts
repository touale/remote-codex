import { expect, it } from 'vitest';
import { emptyTiming } from '../bridge/session';
import type { HistoryPage, ToolItem } from '../bridge/types';
import { mergeHistory } from './history';
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
const chat = () =>
  initialChat(
    session,
    'dev',
    {
      mode: 'agent',
      model: 'model',
      effort: null,
      full_access: false,
      approval_policy: 'on-request',
      reviewer: 'user',
    },
    [],
  );
const tool: ToolItem = {
  id: 'item',
  kind: 'webSearch',
  title: 'Search the web',
  status: 'inProgress',
  output: '',
  changes: [],
};
const page = (tool: ToolItem): HistoryPage => ({
  session,
  next_cursor: null,
  turns: [
    {
      id: 'turn',
      status: 'inProgress',
      timing: emptyTiming(),
      items: [{ id: tool.id, kind: tool.kind, text: '', client_id: null, sent_at: null, phase: null, tool }],
    },
  ],
});

it('restores completed tools during an active turn without regressing to stale running history', () => {
  const running = reduceEvent(chat(), { type: 'tool_changed', turn_id: 'turn', item: tool });
  const complete = {
    ...tool,
    status: 'completed',
    input: 'Rust',
    links: [{ title: 'Rust', url: 'https://www.rust-lang.org/' }],
  };
  const restored = mergeHistory(running, page(complete));
  expect(restored.messages[0].tool).toMatchObject(complete);
  expect(mergeHistory(restored, page(tool)).messages[0].tool).toEqual(restored.messages[0].tool);
});

it('preserves streamed output across missing aggregates and accepts a final aggregate once', () => {
  const command = { ...tool, kind: 'commandExecution', title: 'Command' };
  let state = reduceEvent(chat(), { type: 'tool_changed', turn_id: 'turn', item: command });
  state = reduceEvent(state, { type: 'tool_output', turn_id: 'turn', item_id: tool.id, text: 'hello world' });
  expect(mergeHistory(state, page({ ...command, output: 'hello' })).messages[0].tool?.output).toBe('hello world');
  state = reduceEvent(state, { type: 'tool_changed', turn_id: 'turn', item: { ...command, status: 'completed' } });
  expect(state.messages[0].tool?.output).toBe('hello world');
  expect(mergeHistory(state, page({ ...command, status: 'completed' })).messages[0].tool?.output).toBe('hello world');
  state = reduceEvent(state, {
    type: 'tool_changed',
    turn_id: 'turn',
    item: { ...command, status: 'completed', output: 'final' },
  });
  expect(state.messages[0].tool?.output).toBe('final');
});
