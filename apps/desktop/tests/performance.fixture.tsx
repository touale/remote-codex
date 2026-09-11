import type { Transfer } from '../src/bridge/files';
// Loaded only by the e2e build. Uses the production chat and session reducer.
import { Tooltip } from 'radix-ui';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { Root } from 'react-dom/client';
import { useApplication } from '../src/app/useApplication';
import { useNavigation } from '../src/app/useNavigation';
import { SessionChat } from '../src/chat/SessionChat';
import { DraftChat } from '../src/chat/drafts/DraftChat';
import { initialChat, reduceEvent } from '../src/chat/state';
import { useChats } from '../src/chat/useChats';
import { useFiles } from '../src/files/useFiles';
import { ConnectionTree } from '../src/navigation/ConnectionTree';
import { changeAppPreferences } from '../src/settings/preferences';
import { TransferButton } from '../src/transfers/TransferButton';
import { accept as acceptTransfer } from '../src/transfers/store';
import { useTransfers } from '../src/transfers/useTransfers';
import { useDialog } from '../src/ui/useDialog';

import { emptyStatus } from '../src/bridge/session';

const session = {
  id: 'performance',
  title: 'Long conversation',
  cwd: '/workspace/performance',
  created_at: 1,
  updated_at: 1,
  archived: false,
  state: 'idle',
};
const settings = {
  mode: 'agent' as const,
  model: 'fixture-model',
  effort: 'medium',
  full_access: false,
  reviewer: 'user',
};
const report = (error: unknown) => {
  throw error;
};
const noop = () => {};
const nextFrame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
function Fixture() {
  const app = useApplication();
  const dialog = useDialog();
  const chats = useChats(report, dialog.ask);
  const files = useFiles();
  const nav = useNavigation(app, chats, dialog, files);
  const transfers = useTransfers(files, dialog.ask, report);
  const selected = chats.chats[session.id];
  const [width, setWidth] = useState(840);
  const [result, setResult] = useState('');
  const commits = useRef(0);
  useLayoutEffect(() => {
    commits.current++;
  });
  useEffect(() => {
    const state = initialChat(session, 'fixture', settings, []);
    state.status.limits_updated_at = Date.now() / 1000;
    const markdown =
      '### Implementation\n\nA realistic response with **formatting**, a [reference](https://example.com), and code.\n\n' +
      '| Change | Result |\n| --- | --- |\n' +
      '| Parser | Validated |\n'.repeat(6) +
      '\n```rust\n' +
      'let result = execute_request(input).await?;\n'.repeat(12) +
      '```\n\n- Keep the original state\n- Validate the response\n- Preserve the session\n';
    for (let n = 0; n < 120; n++) {
      const turn = `turn-${String(n).padStart(4, '0')}`;
      state.turns[turn] = {
        id: turn,
        status: 'completed',
        timing: { started_at: n * 60, completed_at: n * 60 + 20, duration_ms: 20000 },
      };
      state.messages.push(
        { id: `user-${n}`, role: 'user', text: `Review change ${n}`, turn, sentAt: n * 60 },
        {
          id: `tool-${n}`,
          role: 'assistant',
          text: '',
          turn,
          tool: {
            id: `tool-${n}`,
            kind: 'commandExecution',
            title: 'Inspect workspace',
            output: 'output\n'.repeat(80),
            status: 'completed',
            changes: [],
          },
        },
        { id: `answer-${n}`, role: 'assistant', text: markdown, turn, complete: true },
      );
    }
    chats.register(
      {
        status: 'open',
        session,
        settings,
        models: [],
        snapshot: {
          goal: null,
          plan: null,
          status: emptyStatus(),
          current_turn: null,
          settings,
          turn: null,
          environment: { status: 'ready' },
          pending: [],
          closed: false,
        },
      },
      'fixture',
    );
    chats.update(session.id, () => state);
  }, []);
  const run = async () => {
    const metrics: Record<string, { median_ms: number; p95_ms: number; root_commits: number }> = {};
    const measure = async (name: string, action: (n: number) => void) => {
      const samples: number[] = [];
      const before = commits.current;
      for (let n = 0; n < 12; n++) {
        await nextFrame();
        const start = performance.now();
        action(n);
        await nextFrame();
        await nextFrame();
        samples.push(performance.now() - start);
      }
      samples.sort((a, b) => a - b);
      metrics[name] = { median_ms: samples[6], p95_ms: samples[11], root_commits: commits.current - before };
    };
    await measure('input', (n) => {
      const input = document.querySelector<HTMLTextAreaElement>('.composer textarea')!;
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(input, `Draft ${n}`);
      input.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await measure('scroll', (n) => {
      const view = document.querySelector<HTMLElement>('.chat-scroll')!;
      view.scrollTop = n % 2 ? view.scrollHeight : view.scrollHeight - 250;
      view.dispatchEvent(new Event('scroll', { bubbles: true }));
    });
    await measure('edge', (n) => {
      const view = document.querySelector<HTMLElement>('.chat-scroll')!;
      const rect = view.getBoundingClientRect();
      view.dispatchEvent(
        new PointerEvent('pointermove', {
          bubbles: true,
          pointerType: 'mouse',
          clientX: n % 2 ? rect.right - 3 : rect.left + rect.width / 2,
          clientY: rect.top + rect.height / 2,
        }),
      );
    });
    await measure('stream', (n) => {
      chats.update(session.id, (state) =>
        reduceEvent(state, {
          type: 'message',
          item_id: 'live',
          turn_id: 'live-turn',
          phase: null,
          text: `Chunk ${n}. `,
          complete: false,
        }),
      );
    });
    const transfer: Transfer = {
      id: 'performance-transfer',
      name: 'project-assets',
      server: 'fixture',
      workspace: session.cwd,
      direction: 'upload',
      destination: '',
      status: 'running',
      bytes: 0,
      total: 1024 * 1024 * 1024,
      files: 20,
      completed: 1,
      skipped: 0,
      message: null,
      error_code: null,
      conflict: null,
      active: true,
      owned: true,
    };
    acceptTransfer(transfer);
    await nextFrame();
    document.querySelector<HTMLButtonElement>('button[aria-label="File transfers"]')!.click();
    await nextFrame();
    await measure('transfer', (n) => {
      acceptTransfer({ ...transfer, bytes: n * 256 * 1024 });
      const input = document.querySelector<HTMLTextAreaElement>('.composer textarea')!;
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(input, `Transfer draft ${n}`);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      document.querySelector<HTMLElement>('.chat-scroll')!.scrollTop += n % 2 ? 40 : -40;
    });
    document.querySelector<HTMLButtonElement>('button[aria-label="Close transfers"]')!.click();
    await measure('resize', (n) => setWidth(840 - n * 8));
    const view = document.querySelector<HTMLElement>('.chat-scroll')!;
    view.scrollTop = view.scrollHeight;
    view.dispatchEvent(new Event('scroll', { bubbles: true }));
    for (const visible of [false, true]) {
      await changeAppPreferences({ five_hour_limit: visible, context_usage: visible });
      await nextFrame();
      await nextFrame();
      if (Math.abs(view.scrollHeight - view.clientHeight - view.scrollTop) > 2)
        throw new Error('Usage details displaced the conversation bottom');
    }
    view.scrollTop = 200;
    view.dispatchEvent(new Event('scroll', { bubbles: true }));
    await changeAppPreferences({ five_hour_limit: false, context_usage: false });
    await nextFrame();
    await nextFrame();
    if (view.scrollTop !== 200) throw new Error('Usage details displaced the reading position');
    await changeAppPreferences({ five_hour_limit: true, context_usage: true });
    await measure('new_session', () => nav.newSession('fixture', session.cwd));
    setResult(JSON.stringify(metrics));
  };
  return (
    <Tooltip.Provider>
      <div style={{ display: 'flex', flexDirection: 'column', width, height: '100vh' }}>
        <button
          disabled={!selected || !app.ready}
          onClick={() => {
            void run();
          }}
        >
          Measure long conversation
        </button>
        <ConnectionTree
          catalog={app.catalog}
          serverHome={null}
          onSelectServer={noop}
          chats={chats.chats}
          selected={null}
          current={nav.target}
          refreshing={false}
          onRefresh={noop}
          hidden
          collapsed={[]}
          onCollapsed={noop}
          onHidden={noop}
          onAddServer={noop}
          onEditServer={noop}
          onRemoveServer={noop}
          onAddWorkspace={noop}
          onRemoveWorkspace={noop}
          onSelectWorkspace={noop}
          onSelectSession={noop}
          onNew={noop}
          onSessionMenu={noop}
          onTerminal={noop}
          onWindow={noop}
          report={report}
        />
        {nav.draftKey ? (
          <DraftChat draftKey={nav.draftKey} store={nav.drafts} submit={nav.submitDraft} />
        ) : (
          <SessionChat
            id={session.id}
            controller={chats}
            onFreshPlan={async () => {}}
            onResume={noop}
            onDiff={noop}
            report={report}
          />
        )}
        <TransferButton actions={transfers} report={report} />
        {result && <pre data-performance-result>{result}</pre>}
      </div>
    </Tooltip.Provider>
  );
}
export function installPerformanceFixture(root: Root) {
  window.addEventListener('remote-codex:performance-fixture', () => root.render(<Fixture />), { once: true });
}
