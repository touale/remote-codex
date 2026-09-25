import { useCallback, useEffect, useRef, useState } from 'react';
import { call, failure } from '../bridge/client';
import type { ToolItem } from '../bridge/types';
import { ErrorText } from '../ui/controls';
import { MarkdownBody } from './MarkdownBody';

/** Mounted only while the tool is expanded; the native session caches the source. */
export function ToolOutput({
  tool,
  report,
  onOpenFile,
}: {
  tool: ToolItem;
  report: (error: unknown) => void;
  onOpenFile?: (href: string) => Promise<void>;
}) {
  const [text, setText] = useState('');
  const [next, setNext] = useState<number | null>(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const active = useRef(false);
  const pending = useRef(false);
  const viewport = useRef<HTMLPreElement>(null);
  const end = useRef<HTMLDivElement>(null);
  const source = useRef(tool.output_source);
  source.current = tool.output_source;
  const deferred = Boolean(tool.output_source);
  const load = useCallback(async (offset: number) => {
    const current = source.current;
    if (!current || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      const chunk = await call('session_tool_output', { id: current.session, source: current, offset });
      if (active.current) {
        setText((previous) => (offset === 0 ? chunk.text : previous + chunk.text));
        setNext(chunk.next_offset);
      }
    } catch (error) {
      if (active.current) setError(failure(error).message);
    } finally {
      pending.current = false;
      if (active.current) setBusy(false);
    }
  }, []);
  useEffect(() => {
    active.current = true;
    if (source.current) void load(0);
    return () => {
      active.current = false;
    };
  }, [load]);
  useEffect(() => {
    if (!deferred || next === null || next === 0 || busy || error) return;
    const more = () => void load(next);
    const view = viewport.current;
    if (tool.kind !== 'reasoning') {
      const check = () => {
        if (!view || view.scrollHeight - view.scrollTop - view.clientHeight <= view.clientHeight) more();
      };
      check();
      if (!view) return;
      const observer = new ResizeObserver(check);
      observer.observe(view);
      view.addEventListener('scroll', check);
      return () => {
        observer.disconnect();
        view.removeEventListener('scroll', check);
      };
    }
    const marker = end.current;
    if (!marker) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) more();
      },
      { root: marker.closest('.chat-scroll'), rootMargin: '0px 0px 300px 0px' },
    );
    observer.observe(marker);
    return () => observer.disconnect();
  }, [deferred, next, busy, error, load, tool.kind]);
  const output = deferred ? text : tool.output;
  if (!deferred && !output) return null;
  return (
    <section aria-label="Tool output">
      {tool.kind === 'reasoning' ? (
        <div className="message-body tool-reasoning">
          <MarkdownBody text={output} report={report} onOpenFile={onOpenFile} />
        </div>
      ) : (
        <>
          <h3>Output</h3>
          {output && <pre ref={viewport}>{output}</pre>}
        </>
      )}
      <div ref={end} />
      {busy && <small role="status">Loading output…</small>}
      <ErrorText message={error} />
      {deferred && error && next !== null && !busy && (
        <button className="subtle" onClick={() => void load(next)}>
          Retry loading output
        </button>
      )}
    </section>
  );
}
