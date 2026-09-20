import { afterEach, expect, it, vi } from 'vitest';
import { emptyTiming } from '../bridge/session';
import { initialChat } from './state';
import { ChatStore } from './store';
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
it('coalesces stream paints without losing order or delaying control events', () => {
  vi.useFakeTimers();
  vi.stubGlobal('requestAnimationFrame', (callback: () => void) => setTimeout(callback, 16));
  vi.stubGlobal('cancelAnimationFrame', clearTimeout);
  const store = new ChatStore();
  for (const id of ['active', 'background'])
    store.set(
      id,
      initialChat(
        { id, title: id, cwd: '/test', created_at: 0, updated_at: 0, archived: false, state: 'idle' },
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
      ),
    );
  const overview = vi.fn();
  const active = vi.fn();
  store.subscribeSummaries(overview);
  store.subscribe('active', active);
  for (let n = 0; n < 100; n++) {
    store.receive('active', {
      type: 'message',
      item_id: 'message',
      turn_id: 'turn',
      phase: null,
      complete: false,
      text: `${n},`,
    });
    store.receive('background', {
      type: 'message',
      item_id: 'other',
      turn_id: 'other-turn',
      phase: null,
      complete: false,
      text: '.',
    });
  }
  expect(active).not.toHaveBeenCalled();
  expect(overview).not.toHaveBeenCalled();
  vi.advanceTimersByTime(16);
  expect(active).toHaveBeenCalledTimes(1);
  expect(store.get('active')?.messages[0].text).toBe(Array.from({ length: 100 }, (_, n) => `${n},`).join(''));
  store.receive('active', {
    type: 'message',
    item_id: 'message',
    turn_id: 'turn',
    phase: 'final_answer',
    complete: true,
    text: 'Final response',
  });
  store.receive('active', { type: 'turn_started', id: 'turn', timing: emptyTiming() });
  store.receive('active', { type: 'approval_requested', request_id: 'approval', description: 'Review this action' });
  expect(active).toHaveBeenCalledTimes(4);
  expect(overview).toHaveBeenCalledTimes(2);
  expect(store.get('active')?.messages[0].text).toBe('Final response');
  expect(store.get('active')?.questions[0].id).toBe('approval');
  store.dispose();
  vi.runAllTimers();
  expect(active).toHaveBeenCalledTimes(4);
});
