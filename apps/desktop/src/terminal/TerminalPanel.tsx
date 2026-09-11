import { FitAddon } from '@xterm/addon-fit';
import { Terminal as Xterm } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { Minus, Plus, Terminal, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { call, listen, openLink } from '../bridge/client';
import type { ShellEvent } from '../bridge/types';
import { IconButton } from '../ui/controls';

export interface TerminalTab {
  id: string;
  workspace: string | null;
  server: string;
  path: string;
  title: string;
  closed: boolean;
}
const waiting = new Map<string, { event: ShellEvent; done: () => void }>();
const consumers = new Map<string, (event: ShellEvent) => Promise<void>>();
listen((event) => {
  if (event.kind !== 'terminal') return;
  const consumer = consumers.get(event.id);
  if (consumer) return consumer(event.event);
  return new Promise<void>((done) => {
    waiting.set(event.id, { event: event.event, done });
  });
});

export function TerminalPanel({
  tabs,
  visible: panelVisible,
  server,
  selected,
  dark,
  onSelect,
  onNew,
  onClose,
  onHide,
  onEnded,
  report,
}: {
  tabs: TerminalTab[];
  visible: boolean;
  server: string | null;
  selected: string | null;
  dark: boolean;
  onSelect: (id: string) => void;
  onNew: () => void;
  onClose: (id: string) => void;
  onHide: () => void;
  onEnded: (id: string) => void;
  report: (error: unknown) => void;
}) {
  const visible = tabs.filter((tab) => tab.server === server);
  const active = visible.find((tab) => tab.id === selected)?.id ?? visible.at(-1)?.id;
  return (
    <section className="terminal-panel" aria-label="Remote terminal">
      <div className="pane-toolbar">
        <Terminal size={14} />
        <span>Terminal</span>
        <div className="terminal-tabs">
          {visible.map((tab) => (
            <div key={tab.id} className={`terminal-tab ${active === tab.id ? 'selected' : ''}`}>
              <button title={`${tab.server} · ${tab.path}`} onClick={() => onSelect(tab.id)}>
                {tab.title}
                {tab.closed ? ' · Ended' : ''}
              </button>
              <IconButton label="Close terminal" onClick={() => onClose(tab.id)}>
                <X size={12} />
              </IconButton>
            </div>
          ))}
        </div>
        <div className="spacer" />
        <IconButton label="New terminal" disabled={!server} onClick={onNew}>
          <Plus size={15} />
        </IconButton>
        <IconButton label="Minimize terminal" onClick={onHide}>
          <Minus size={15} />
        </IconButton>
      </div>
      <div className="terminal-surfaces">
        {tabs.map((tab) => (
          <TerminalView
            key={tab.id}
            tab={tab}
            active={panelVisible && tab.id === active}
            dark={dark}
            report={report}
            onEnded={onEnded}
          />
        ))}
      </div>
    </section>
  );
}
function TerminalView({
  tab,
  active,
  dark,
  report,
  onEnded,
}: {
  tab: TerminalTab;
  onEnded: (id: string) => void;
  active: boolean;
  dark: boolean;
  report: (error: unknown) => void;
}) {
  const element = useRef<HTMLDivElement>(null);
  const terminal = useRef<Xterm | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const [closed, setClosed] = useState(false);
  useEffect(() => {
    if (!element.current) return;
    const term = new Xterm({
      fontFamily: 'SFMono-Regular, Menlo, monospace',
      fontSize: 12,
      cursorBlink: true,
      scrollback: 3000,
      allowProposedApi: false,
    });
    const addon = new FitAddon();
    term.loadAddon(addon);
    term.open(element.current);
    terminal.current = term;
    fit.current = addon;
    let ended = false;
    const consume = async (event: ShellEvent) => {
      if (event.type === 'output') await new Promise<void>((done) => term.write(new Uint8Array(event.bytes), done));
      else {
        ended = true;
        setClosed(true);
        onEnded(tab.id);
        term.write('\r\n\x1b[90mConnection ended. Open a new terminal to reconnect.\x1b[0m\r\n');
      }
    };
    consumers.set(tab.id, consume);
    const queued = waiting.get(tab.id);
    if (queued) void consume(queued.event).finally(queued.done);
    waiting.delete(tab.id);
    const input = term.onData((data) => {
      if (!ended) void call('terminal_input', { id: tab.id, bytes: [...new TextEncoder().encode(data)] }).catch(report);
    });
    const resize = term.onResize((size) => {
      if (!ended)
        void call('terminal_resize', {
          id: tab.id,
          columns: Math.max(2, size.cols),
          rows: Math.max(2, size.rows),
        }).catch(report);
    });
    const observer = new ResizeObserver(() => {
      if (element.current && element.current.clientWidth > 0 && element.current.clientHeight > 0) addon.fit();
    });
    observer.observe(element.current);
    term.registerLinkProvider({
      provideLinks: (line, callback) => {
        const text = term.buffer.active.getLine(line - 1)?.translateToString() ?? '';
        callback(
          [...text.matchAll(/https?:\/\/[^\s<>]+/g)].map((match) => ({
            text: match[0],
            range: { start: { x: match.index! + 1, y: line }, end: { x: match.index! + match[0].length, y: line } },
            activate: () => {
              void openLink(match[0]).catch(report);
            },
          })),
        );
      },
    });
    return () => {
      consumers.delete(tab.id);
      waiting.get(tab.id)?.done();
      waiting.delete(tab.id);
      observer.disconnect();
      input.dispose();
      resize.dispose();
      term.dispose();
      terminal.current = null;
    };
  }, [tab.id, report, onEnded]);
  useEffect(() => {
    if (terminal.current)
      terminal.current.options.theme = {
        background: dark ? '#212121' : '#FFFFFF',
        foreground: dark ? '#ECECEC' : '#0D0D0D',
        cursor: dark ? '#ECECEC' : '#0D0D0D',
        selectionBackground: dark ? '#494949' : '#DDDDDD',
      };
  }, [dark]);
  useEffect(() => {
    if (active) {
      fit.current?.fit();
      terminal.current?.focus();
    }
  }, [active]);
  return (
    <div className="terminal-view" style={{ display: active ? 'flex' : 'none' }}>
      {closed && <div className="terminal-ended">Disconnected</div>}
      <div className="xterm-host" ref={element} />
    </div>
  );
}
