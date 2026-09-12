import { Columns2, Rows3 } from 'lucide-react';
import { memo, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { parseChanges } from './patch';
import './changes.css';

export const ChangeView = memo(function ChangeView({
  diff,
  standalone = false,
}: {
  diff: string;
  standalone?: boolean;
}) {
  const [split, setSplit] = useState(standalone);
  const scroll = useRef<HTMLDivElement>(null);
  const anchor = useRef<{ selector: string; offset: number } | null>(null);
  const parsed = useMemo(() => {
    try {
      return parseChanges(diff);
    } catch {
      return [];
    }
  }, [diff]);
  const changeLayout = (value: boolean) => {
    const area = scroll.current;
    if (area) {
      const nodes = [...area.querySelectorAll<HTMLElement>('.change-row')];
      const node = [...nodes].reverse().find((node) => node.offsetTop <= area.scrollTop) ?? nodes[0];
      if (node)
        anchor.current = {
          selector: node.dataset.after
            ? `[data-after="${node.dataset.after}"]`
            : `[data-before="${node.dataset.before}"]`,
          offset: area.scrollTop - node.offsetTop,
        };
    }
    setSplit(value);
  };
  useLayoutEffect(() => {
    const area = scroll.current;
    const saved = anchor.current;
    const node = saved ? area?.querySelector<HTMLElement>(saved.selector) : null;
    if (area && node && saved) area.scrollTop = node.offsetTop + saved.offset;
    anchor.current = null;
  }, [split]);
  return (
    <section className="change-view" aria-label="File changes">
      <div className="change-toolbar">
        <span>
          {parsed.length
            ? 'Changed sections'
            : diff.trim()
              ? 'Original patch · Split view unavailable'
              : 'No changes recorded'}
        </span>
        {!!parsed.length && (
          <div className="change-layout" role="group" aria-label="Diff layout">
            <button aria-pressed={!split} onClick={() => changeLayout(false)}>
              <Rows3 size={13} />
              Unified
            </button>
            <button aria-pressed={split} onClick={() => changeLayout(true)}>
              <Columns2 size={13} />
              Split
            </button>
          </div>
        )}
      </div>
      {split && !!parsed.length && (
        <div className="change-sides">
          <span>Before</span>
          <span>After</span>
        </div>
      )}
      <div className="change-scroll" ref={scroll} data-layout={split ? 'split' : 'unified'}>
        {parsed.length ? (
          parsed.map((hunk, index) => (
            <section key={index} data-hunk={index}>
              <div className="change-hunk">
                {hunk.label}
                {hunk.omitted && <span>Unchanged lines omitted</span>}
              </div>
              {(split ? hunk.split : hunk.unified).map((row, i) => (
                <div key={i} className="change-row" data-before={row.before?.number} data-after={row.after?.number}>
                  <span className="change-number">{row.before?.number}</span>
                  {split ? (
                    <>
                      <code className={row.changed ? (row.before ? 'deletion' : 'change-blank') : ''}>
                        {row.before?.text || ' '}
                      </code>
                      <span className="change-number">{row.after?.number}</span>
                      <code className={row.changed ? (row.after ? 'addition' : 'change-blank') : ''}>
                        {row.after?.text || ' '}
                      </code>
                    </>
                  ) : (
                    <>
                      <span className="change-number">{row.after?.number}</span>
                      <code className={row.changed ? (row.before ? 'deletion' : 'addition') : ''}>
                        <span className="change-sign">{row.changed ? (row.before ? '−' : '+') : ' '}</span>
                        {(row.after ?? row.before)?.text || ' '}
                      </code>
                    </>
                  )}
                </div>
              ))}
              {hunk.noNewline && <div className="change-hunk">No newline at end of file</div>}
            </section>
          ))
        ) : (
          <pre className="change-raw">{diff}</pre>
        )}
      </div>
    </section>
  );
});
