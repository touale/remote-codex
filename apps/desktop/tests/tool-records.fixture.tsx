import { useState } from 'react';
import type { ToolItem } from '../src/bridge/types';
import { Conversation } from '../src/chat/Conversation';
import type { FileChange } from '../src/files/useFiles';

const items: ToolItem[] = [
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
    title: 'File changes · 1 file',
    status: 'completed',
    output: '',
    changes: [{ path: 'src/main.rs', diff: '@@ -1 +1 @@\n-old\n+new' }],
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

export function ToolRecordsFixture() {
  const [tools, setTools] = useState(items);
  const [diff, setDiff] = useState<FileChange | null>(null);
  const [error, setError] = useState('');
  return (
    <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', gap: 10 }}>
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
      {diff && (
        <output aria-label="Opened diff">
          {diff.path}
          {'\n'}
          {diff.diff}
        </output>
      )}
      <div className="chat-scroll" aria-label="Tool records">
        <div className="messages">
          <Conversation
            messages={tools.map((tool) => ({ id: tool.id, role: 'assistant', text: '', tool }))}
            turns={{}}
            onDiff={setDiff}
            report={(error) => setError(String((error as { message: string }).message))}
          />
        </div>
      </div>
    </div>
  );
}
