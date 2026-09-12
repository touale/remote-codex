import { Tooltip } from 'radix-ui';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import { emptyTiming, type TurnState } from '../bridge/session';
import { Conversation } from './Conversation';
import type { Message } from './state';

it('keeps empty stopped turns before new progress, including history loaded after the active turn', () => {
  const earlier: TurnState = {
    id: '01a09170-0000-7000-8000-000000000000',
    status: 'interrupted',
    timing: emptyTiming(),
  };
  const stopped: TurnState = {
    id: '01a09171-be5e-7ef3-aa04-8fc1458a0adf',
    status: 'interrupted',
    timing: { started_at: 1789146545, completed_at: 1789146718, duration_ms: 172185 },
  };
  const active: TurnState = {
    id: '01a09174-ab4d-7422-ae6e-174e6947cef7',
    status: 'inProgress',
    timing: { ...emptyTiming(), started_at: 1789146737 },
  };
  const render = (messages: Message[]) =>
    renderToStaticMarkup(
      <Tooltip.Provider>
        <Conversation
          turns={{ [active.id]: active, [stopped.id]: stopped, [earlier.id]: earlier }}
          messages={messages}
          onDiff={() => {}}
          report={() => {}}
        />
      </Tooltip.Provider>,
    );
  const order = (html: string) => [...html.matchAll(/data-turn-id="([^"]+)"/g)].map((match) => match[1]);
  expect(order(render([]))).toEqual([earlier.id, stopped.id, active.id]);

  const progress: Message = { id: 'progress', turn: active.id, role: 'assistant', text: 'Current progress' };
  const pending: Message = { id: 'pending', role: 'user', text: 'Pending follow-up' };
  const html = render([progress, pending]);
  expect(order(html)).toEqual([earlier.id, stopped.id, active.id]);
  expect(html).toContain('Worked for 2m 52s');
  expect(html.lastIndexOf('· Stopped')).toBeLessThan(html.indexOf('Current progress'));
  expect(html.indexOf('Current progress')).toBeLessThan(html.indexOf('Pending follow-up'));

  // Late output still belongs to its original turn, even after new progress arrives.
  const updated = render([
    { ...progress, text: 'Current progress continues' },
    pending,
    { id: 'late', turn: stopped.id, role: 'assistant', text: 'Earlier output' },
  ]);
  expect(order(updated)).toEqual([earlier.id, stopped.id, active.id]);
  expect(updated.indexOf('Earlier output')).toBeLessThan(updated.lastIndexOf('· Stopped'));
  expect(updated.lastIndexOf('· Stopped')).toBeLessThan(updated.indexOf('Current progress continues'));
});
