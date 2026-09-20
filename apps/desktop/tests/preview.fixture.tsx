import { useRef, useState } from 'react';
import { ContentWindow } from '../src/app/ContentWindow';
import EditorPane from '../src/files/Editor';
import { useFiles } from '../src/files/useFiles';
import type { useApplication } from '../src/app/useApplication';

export function PreviewFixture({ app }: { app: Pick<ReturnType<typeof useApplication>, 'dark' | 'report'> }) {
  const files = useFiles();
  const [standalone, setStandalone] = useState(false);
  const closeHandler = useRef<() => void>(() => {});
  const [error, setError] = useState('');
  const context = { id: 'preview-fixture', server: 'fixture', path: '/project', kind: 'workspace' as const };
  const run = (task: Promise<unknown>) => void task.catch((error) => setError(String(error)));
  return (
    <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
      <div>
        <button onClick={() => setStandalone(!standalone)}>Toggle content window</button>
        {['paper.pdf', 'diagram.svg', 'notes/readme.md', 'broken.pdf', 'large.pdf', 'slow.pdf'].map((path) => (
          <button
            key={path}
            aria-label={`Open ${path}`}
            onClick={() => run(files.open(context.id, path, context.server, context.path))}
          >
            Open {path}
          </button>
        ))}
        <button
          onClick={() => {
            const buffer = files.get(files.selected[context.id]);
            if (buffer) files.update(buffer.key, { text: buffer.text + '\n\nDraft retained' });
          }}
        >
          Edit draft
        </button>
      </div>
      <output>{error}</output>
      {standalone ? (
        <ContentWindow
          app={{
            dark: app.dark,
            prompts: [],
            answer: () => {},
            report: (error) => setError(String(error)),
            error,
            setError,
            closeHandler,
          }}
          target={{
            kind: 'file',
            context: 'preview-fixture',
            transfer: '',
            document: {
              kind: 'text',
              server: 'fixture',
              root: '/project',
              path: 'retry.md',
              text: '',
              original: '',
              revision: '',
            },
          }}
        />
      ) : (
        <EditorPane
          context={context}
          tabs={files.tabs}
          selected={files.selected[context.id]}
          dark={app.dark}
          diff={null}
          onSelect={(key) => files.select(context.id, key)}
          onClose={files.close}
          onRetry={(key) => run(files.retry(key))}
          onHide={() => {}}
          onCloseDiff={() => {}}
          onSave={(key) => run(files.save(key))}
          onChange={(key, text) => files.update(key, { text })}
          onMode={(key, mode, view) => files.update(key, { mode, ...(view ? { view } : {}) })}
          onPreviewView={files.setPreviewView}
          onOpenFile={files.open}
          onDismissConflict={(key) => files.update(key, { conflict: undefined })}
          onDiscardConflict={files.discard}
        />
      )}
    </div>
  );
}
