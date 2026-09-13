import { expect, it, vi } from 'vitest';
import { call } from '../bridge/client';
import { ChatStore } from '../chat/store';
import { prepareSession } from './sessionOpening';

vi.mock('../bridge/client', () => ({
  call: vi.fn(),
  failure: (error: unknown) => error,
  operationId: () => 'operation',
}));

it('preserves confirmed choices when trust, MCP conflicts and takeover occur in sequence', async () => {
  vi.mocked(call)
    .mockResolvedValueOnce({ status: 'trust', preparation: 'prepared', names: ['tools'] } as never)
    .mockRejectedValueOnce({ code: 'MCP_NAME_CONFLICT', message: 'Duplicate project tools.' })
    .mockRejectedValueOnce({ code: 'SESSION_IN_USE', message: 'Already open.' })
    .mockResolvedValueOnce({ status: 'open', session: { id: 'session' } } as never);
  const store = new ChatStore();
  const ask = vi
    .fn()
    .mockResolvedValueOnce('Trust and continue')
    .mockResolvedValueOnce('Use local')
    .mockResolvedValueOnce('Take over');
  const register = vi.fn();
  const workspace = { id: 'workspace', server: 'dev', path: '/project' };
  const opened = await prepareSession(
    {
      connect: async () => workspace,
      chats: { store, register, loadHistory: vi.fn() },
      dialog: { ask },
      refresh: async () => {},
      report: vi.fn(),
    },
    'dev',
    'session',
    '/project',
    vi.fn(),
  );
  expect(ask.mock.calls.map(([prompt]) => prompt.title)).toEqual([
    'Trust project MCP servers?',
    'Choose MCP configuration',
    'Take over this session?',
  ]);
  expect(call).toHaveBeenNthCalledWith(2, 'session_trust', {
    operationId: 'operation',
    preparation: 'prepared',
    accept: true,
  });
  expect(call).toHaveBeenLastCalledWith('session_open', {
    operationId: 'operation',
    input: { server: 'dev', path: '/project', resume: 'session', takeover: true, mcp_source: 'local' },
  });
  expect(opened).toEqual({ workspace, id: 'session' });
  expect(register).toHaveBeenCalledOnce();
  store.dispose();
});

it('does not offer takeover when resuming a new-window target that became occupied', async () => {
  vi.mocked(call).mockReset().mockRejectedValueOnce({ code: 'SESSION_IN_USE', message: 'Already open.' });
  const store = new ChatStore();
  const ask = vi.fn();
  await expect(
    prepareSession(
      {
        connect: async () => ({ id: 'workspace', server: 'dev', path: '/project' }),
        chats: { store, register: vi.fn(), loadHistory: vi.fn() },
        dialog: { ask },
        refresh: async () => {},
        report: vi.fn(),
      },
      'dev',
      'session',
      '/project',
      vi.fn(),
      false,
    ),
  ).rejects.toMatchObject({ code: 'SESSION_IN_USE' });
  expect(ask).not.toHaveBeenCalled();
  expect(call).toHaveBeenCalledOnce();
  store.dispose();
});

it.each(['MCP_NAME_CONFLICT', 'SESSION_IN_USE'])('stops if %s remains after confirmation', async (code) => {
  vi.mocked(call).mockReset().mockRejectedValue({ code, message: 'Still unavailable.' });
  const store = new ChatStore();
  const ask = vi.fn(async () => (code === 'MCP_NAME_CONFLICT' ? 'Use local' : 'Take over'));
  const register = vi.fn();
  await expect(
    prepareSession(
      {
        connect: async () => ({ id: 'workspace', server: 'dev', path: '/project' }),
        chats: { store, register, loadHistory: vi.fn() },
        dialog: { ask },
        refresh: async () => {},
        report: vi.fn(),
      },
      'dev',
      'session',
      '/project',
      vi.fn(),
    ),
  ).rejects.toMatchObject({ code });
  expect(ask).toHaveBeenCalledOnce();
  expect(call).toHaveBeenCalledTimes(2);
  expect(register).not.toHaveBeenCalled();
  store.dispose();
});
