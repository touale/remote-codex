// Test-build-only surfaces composed from the production controls and renderers.
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { Tooltip } from 'radix-ui';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import type { Root } from 'react-dom/client';
import { useApplication } from '../src/app/useApplication';
import type { Buffer } from '../src/files/tabs';
import { SettingsDialog } from '../src/settings/SettingsDialog';
import { Disclosure } from '../src/ui/Disclosure';
import { SelectField } from '../src/ui/SelectField';
import { Modal } from '../src/ui/controls';
import { ToolRecordsFixture } from './tool-records.fixture';
const Editor = lazy(() => import('../src/files/Editor'));
const choices = Array.from({ length: 35 }, (_, i) => ({
  value: String(i),
  label: `Workspace ${String(i).padStart(2, '0')} · /workspace/a-long-project-path`,
  disabled: i === 33,
}));
const text = ('const longLine = "' + 'content '.repeat(60) + '";\n').repeat(100);
const buffer: Buffer = {
  key: 'fixture',
  context: 'fixture',
  server: 'fixture',
  root: '/workspace',
  path: 'test.ts',
  text,
  original: text,
  revision: 'fixture',
  saving: false,
};
const noop = () => {};
function TerminalSurface() {
  const element = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const terminal = new Terminal({ cols: 55, rows: 8, scrollback: 200 });
    terminal.open(element.current!);
    terminal.write('Terminal history\r\n'.repeat(100));
    return () => terminal.dispose();
  }, []);
  return <div id="fixture-terminal" ref={element} style={{ height: 160 }} />;
}
function Fixture() {
  const app = useApplication();
  const [selected, setSelected] = useState('0');
  const [settings, setSettings] = useState(false);
  const [dialog, setDialog] = useState(false);
  const [editor, setEditor] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [tools, setTools] = useState(false);
  return (
    <Tooltip.Provider>
      <div style={{ padding: 24, height: '100%', display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={{ display: 'flex', gap: 8 }}>
          <button onClick={() => setSettings(true)}>Fixture settings</button>
          <button onClick={() => setDialog(true)}>Fixture modal</button>
          <button onClick={() => setEditor(!editor)}>Fixture editor</button>
          <button onClick={() => setConflict(true)}>Fixture conflict</button>
          <button onClick={() => setTools(!tools)}>Fixture tools</button>
        </div>
        {tools ? (
          <ToolRecordsFixture />
        ) : editor ? (
          <Suspense fallback={<p>Loading editor…</p>}>
            <Editor
              context={{ id: 'fixture', server: buffer.server, path: buffer.root, kind: 'workspace' }}
              tabs={[
                {
                  ...buffer,
                  status: 'ready',
                  conflict: conflict
                    ? { path: buffer.path, text: 'remote\n'.repeat(100), revision: 'remote' }
                    : undefined,
                },
              ]}
              dark={app.dark}
              diff={null}
              onSelect={noop}
              onClose={noop}
              onRetry={noop}
              onSave={noop}
              onChange={noop}
              onHide={noop}
              onCloseDiff={noop}
              onDismissConflict={() => setConflict(false)}
              onDiscardConflict={noop}
            />
          </Suspense>
        ) : (
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 20, minHeight: 0 }}>
            <div>
              <SelectField label="Fixture choice" options={choices} value={selected} onValueChange={setSelected} />
              <output id="selected-choice">{selected}</output>
              <Disclosure title="Advanced options">
                <input aria-label="Retained input" defaultValue="kept" />
              </Disclosure>
              <div id="fixture-native" style={{ height: 180, overflow: 'auto', border: '1px solid var(--border)' }}>
                <textarea
                  id="fixture-input"
                  style={{ width: 230, height: 70 }}
                  defaultValue={'Long input\n'.repeat(25)}
                />
                <pre id="fixture-code" style={{ width: 260, overflow: 'auto' }}>
                  {'horizontal '.repeat(100)}
                </pre>
                <div style={{ height: 400 }}>Long content</div>
              </div>
              <div id="fixture-short" style={{ height: 45, overflow: 'auto' }}>
                Short content
              </div>
            </div>
            <div>
              <TerminalSurface />
              <select id="fixture-multiple" multiple size={4}>
                {choices.map((choice) => (
                  <option key={choice.value}>{choice.label}</option>
                ))}
              </select>
            </div>
          </div>
        )}
        {settings && (
          <SettingsDialog
            preferences={app.preferences}
            onPreferences={app.changePreferences}
            onClose={() => setSettings(false)}
            workspace={null}
            session={null}
            catalog={{
              servers: [],
              sessions: [],
              live: [],
              open_session_ids: [],
              workspaces: choices.map((choice) => ({
                server: choice.label.split(' · ')[0],
                server_id: choice.value,
                path: '/workspace/long-path',
                used_at: 1,
              })),
            }}
          />
        )}
        {dialog && (
          <Modal title="Scrollable dialog" onClose={() => setDialog(false)}>
            <div style={{ height: 1200 }}>Long modal content</div>
          </Modal>
        )}
      </div>
    </Tooltip.Provider>
  );
}
export function installControlsFixture(root: Root) {
  window.addEventListener('remote-codex:controls-fixture', () => root.render(<Fixture />), { once: true });
}
