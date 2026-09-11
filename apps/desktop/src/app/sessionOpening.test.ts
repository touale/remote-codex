import { expect, it, vi } from 'vitest';
import { call } from '../bridge/client';
import { ChatStore } from '../chat/store';
import { prepareSession } from './sessionOpening';

vi.mock('../bridge/client', () => ({
  call: vi.fn(),
  failure: (error: unknown) => error,
  operationId: () => 'operation',
}));

it('resolves MCP conflicts by error code even when the message changes', async () => {
  vi.mocked(call)
    .mockRejectedValueOnce({ code: 'MCP_NAME_CONFLICT', message: 'Duplicate project tools.' })
    .mockResolvedValueOnce({ status: 'open', session: { id: 'session' } } as never);
  const store = new ChatStore();
  const ask = vi.fn(async () => 'Use local');
  const register = vi.fn();
  const workspace = { id: 'workspace', server: 'dev', path: '/project' };
  const opened = await prepareSession(
    {
      connect: async () => workspace,
      chats: { store, register, loadHistory: vi.fn() },
      dialog: { ask },
      finishOperation: vi.fn(),
      refresh: async () => {},
      report: vi.fn(),
    },
    'dev',
    null,
    '/project',
    vi.fn(),
  );
  expect(ask).toHaveBeenCalledOnce();
  expect(call).toHaveBeenLastCalledWith('session_open', {
    operationId: 'operation',
    input: { server: 'dev', path: '/project', resume: null, takeover: false, mcp_source: 'local' },
  });
  expect(opened).toEqual({ workspace, id: 'session' });
  expect(register).toHaveBeenCalledOnce();
  store.dispose();
});
