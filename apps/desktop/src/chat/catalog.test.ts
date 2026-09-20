import { expect, it } from 'vitest';
import type { CachedSession } from '../bridge/types';
import { initialChat } from './state';
import { mergeSessions } from './catalog';

const cached: CachedSession = {
  server_id: 'server-id',
  server: 'dev',
  session: {
    id: 'session',
    title: 'Cached title',
    cwd: '/workspace',
    created_at: 100,
    updated_at: 200,
    archived: false,
    state: 'idle',
  },
};
const chat = initialChat(
  { ...cached.session, title: 'Live title', updated_at: 150, archived: true },
  'dev',
  { mode: 'agent', model: 'model', effort: null, full_access: false, approval_policy: 'on-request', reviewer: 'user' },
  [],
);

it.each([
  { closed: false, updated_at: 150, title: 'Live title', archived: true, activity: 200 },
  { closed: false, updated_at: 300, title: 'Live title', archived: true, activity: 300 },
  { closed: false, updated_at: NaN, title: 'Live title', archived: true, activity: 200 },
  { closed: true, updated_at: 300, title: 'Cached title', archived: false, activity: 200 },
])('merges live metadata without regressing activity: %j', ({ closed, updated_at, title, archived, activity }) => {
  const sessions = mergeSessions([cached], {
    session: { ...chat, closed, session: { ...chat.session, updated_at } },
  });
  expect(sessions).toEqual([{ ...cached, session: { ...cached.session, title, archived, updated_at: activity } }]);
  expect(cached.session.title).toBe('Cached title');
  expect(cached.session.archived).toBe(false);
});

it('retains cached sessions and adds missing live or closed sessions', () => {
  for (const closed of [false, true]) {
    const session = { ...chat.session, id: 'missing' };
    expect(mergeSessions([cached], { missing: { ...chat, closed, session } })).toEqual([
      cached,
      { server_id: '', server: 'dev', session },
    ]);
  }
});
