import { call, failure, operationId } from '../bridge/client';
import type { CurrentWorkspace } from '../bridge/files';
import type { SessionOpened } from '../bridge/types';
import type { useChats } from '../chat/useChats';
import type { useDialog } from '../ui/useDialog';

export interface Opened {
  workspace: CurrentWorkspace;
  id: string;
}
interface Services {
  connect: (server: string, path: string) => Promise<CurrentWorkspace>;
  chats: Pick<ReturnType<typeof useChats>, 'store' | 'register' | 'loadHistory'>;
  dialog: Pick<ReturnType<typeof useDialog>, 'ask'>;
  refresh: () => Promise<void>;
  report: (error: unknown) => void;
}
export async function prepareSession(
  { connect, chats, dialog, refresh, report }: Services,
  server: string,
  resume: string | null,
  path: string,
  onPhase: (phase: 'history') => void,
  allowTakeover = true,
): Promise<Opened | null> {
  const current = await connect(server, path);
  const existing = resume ? chats.store.get(resume) : undefined;
  if (resume && existing && !existing.closed) {
    if (!existing.historyReady) {
      onPhase('history');
      await chats.loadHistory(resume);
    }
    return { workspace: current, id: resume };
  }
  let takeover = false;
  let source: string | null = null;
  let result: SessionOpened;
  for (;;) {
    try {
      result = await call('session_open', {
        operationId: operationId(),
        input: { server, path: current.path, resume, takeover, mcp_source: source },
      });
      if (result.status === 'trust') {
        const answer = await dialog.ask({
          title: 'Trust project MCP servers?',
          message: `${server} · ${current.path}\n\n${result.names.join(', ')}\n\nThese commands run on the remote server.`,
          choices: ['Trust and continue'],
        });
        const opened = await call('session_trust', {
          operationId: operationId(),
          preparation: result.preparation,
          accept: Boolean(answer),
        });
        if (!opened) return null;
        result = opened;
      }
      break;
    } catch (error) {
      const issue = failure(error);
      if (issue.code === 'SESSION_IN_USE' && allowTakeover && !takeover) {
        const answer = await dialog.ask({
          title: 'Take over this session?',
          message: 'Another Remote Codex process controls this session. Taking over closes that frontend connection.',
          choices: ['Take over'],
        });
        if (!answer) return null;
        takeover = true;
      } else if (issue.code === 'MCP_NAME_CONFLICT' && source === null) {
        const answer = await dialog.ask({
          title: 'Choose MCP configuration',
          message: 'Local and remote MCP servers have conflicting names.',
          choices: ['Use remote', 'Use local'],
        });
        if (!answer) return null;
        source = answer === 'Use remote' ? 'remote' : 'local';
      } else throw error;
    }
  }
  if (result.status !== 'open') return null;
  chats.register(result, server, Boolean(resume));
  if (resume) {
    onPhase('history');
    await chats.loadHistory(result.session.id);
  }
  void refresh().catch(report);
  return { workspace: current, id: result.session.id };
}
