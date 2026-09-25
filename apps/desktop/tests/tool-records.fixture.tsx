import { useState } from 'react';
import type { ToolItem } from '../src/bridge/types';
import { Conversation } from '../src/chat/Conversation';
import { useFiles } from '../src/files/useFiles';
import { EditorArea, hasEditorContent } from '../src/app/ResourcePanels';
import type { useApplication } from '../src/app/useApplication';

const context = { id: 'fixture', server: 'fixture', path: '/workspace', kind: 'workspace' as const };
const nav = { fileContext: context, workspace: null, target: null, serverHome: null };

const items: ToolItem[] = [
  {
    id: 'history-reasoning',
    kind: 'reasoning',
    title: 'Historical reasoning',
    status: 'completed',
    output: '',
    changes: [],
    output_source: { session: 'fixture', turn: 'turn', cursor: null, item: 'history-reasoning' },
  },
  {
    id: 'history-output',
    kind: 'mcpToolCall',
    title: 'Historical output',
    status: 'completed',
    output: '',
    changes: [],
    output_source: { session: 'fixture', turn: 'turn', cursor: null, item: 'history-output' },
  },
  {
    id: 'search',
    kind: 'webSearch',
    title: 'Search the web · Rust async cancellation',
    status: 'completed',
    input: 'Rust async cancellation\nRust task cancellation',
    output: '',
    changes: [],
    links: [
      {
        title: 'The Rust Programming Language',
        url: 'https://doc.rust-lang.org/book/',
        description: 'Official documentation for Rust.',
      },
    ],
  },

  {
    id: 'command',
    kind: 'commandExecution',
    title: 'cargo check --workspace',
    status: 'inProgress',
    input: 'cargo check --workspace\n\nDirectory: /workspace/project',
    output: 'Checking project…',
    changes: [],
  },
  {
    id: 'patch',
    kind: 'fileChange',
    title: 'File changes · 2 files',
    status: 'completed',
    output: '',
    changes: [
      { path: 'src/main.rs', diff: '@@ -1 +1 @@\n-old\n+new' },
      { path: 'src/other.rs', diff: '@@ -1 +1 @@\n-before\n+after' },
    ],
  },
  {
    id: 'mcp',
    kind: 'mcpToolCall',
    title: 'docs · search',
    status: 'failed',
    input: '{\n  "query": "cancellation"\n}',
    output: 'Partial result\n\nError: Request timed out',
    changes: [],
  },

  {
    id: 'reasoning',
    kind: 'reasoning',
    title: 'Reasoning summary',
    status: 'completed',
    output: '**Compare cancellation behavior**\n\nCheck the documented task lifecycle.',
    changes: [],
  },
  { id: 'empty', kind: 'reasoning', title: 'reasoning', status: 'completed', output: '', changes: [] },
];

export function ToolRecordsFixture({ app }: { app: ReturnType<typeof useApplication> }) {
  const [tools, setTools] = useState(items);
  const files = useFiles();
  const [error, setError] = useState('');
  const report = (error: unknown) => setError(String((error as { message: string }).message));
  return (
    <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', gap: 10 }}>
      <button
        onClick={() =>
          setTools((previous) =>
            previous.map((tool) =>
              tool.id === 'history-output' && tool.output_source
                ? { ...tool, output_source: { ...tool.output_source, cursor: 'revised-page' } }
                : tool,
            ),
          )
        }
      >
        Repage tool output
      </button>
      <button
        style={{ alignSelf: 'flex-start' }}
        onClick={() =>
          setTools((previous) =>
            previous.map((tool) =>
              tool.id === 'command'
                ? {
                    ...tool,
                    status: 'completed',
                    input: tool.input + '\nExit code: 0',
                    output: 'Checking workspace crate\n'.repeat(80) + 'Finished successfully.',
                  }
                : tool,
            ),
          )
        }
      >
        Finish tool output
      </button>
      {error && <div role="alert">{error}</div>}
      {hasEditorContent(nav, files) && (
        <div style={{ display: 'flex', height: 260, flexShrink: 0 }}>
          <EditorArea
            app={{ ...app, report }}
            nav={nav}
            files={files}
            fileActions={{
              save: async () => {},
              close: async (key) => {
                files.close(key);
                return true;
              },
            }}
          />
        </div>
      )}
      <div className="chat-scroll" aria-label="Tool records">
        <div className="messages">
          <Conversation
            messages={tools.map((tool) => ({ id: tool.id, role: 'assistant', text: '', tool }))}
            turns={{}}
            onDiff={(change, separate) => {
              if (separate) void files.openDiffWindow(context, change).catch(report);
              else {
                files.setDiff({ ...change, workspace: context.id });
                app.changePreferences({ editor_visible: true });
              }
            }}
            report={report}
          />
        </div>
      </div>
    </div>
  );
}
