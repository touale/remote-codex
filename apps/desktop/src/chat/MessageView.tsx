import { ChevronDown, FileDiff, Terminal } from 'lucide-react';
import { memo, useState } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { openLink } from '../bridge/client';
import type { FileChange } from '../files/useFiles';
import type { Message } from './state';
import { clockTime, fullTime } from './time';
export type PlanChoice = 'implement' | 'fresh' | 'revise';
export const MessageView = memo(function MessageView({
  message,
  onDiff,
  report,
  onPlan,
  activePlan,
  revisingPlan,
}: {
  message: Message;
  onPlan?: (message: Message, choice: PlanChoice) => void;
  activePlan?: boolean;
  revisingPlan?: boolean;
  onDiff: (change: FileChange) => void;
  report: (error: unknown) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  if (message.tool)
    return (
      <details className="tool" open={expanded} onToggle={(event) => setExpanded(event.currentTarget.open)}>
        <summary>
          <Terminal size={14} />
          <span>{message.tool.title}</span>
          <small>{message.tool.status}</small>
          <ChevronDown size={14} />
        </summary>
        {expanded && message.tool.output && <pre>{message.tool.output}</pre>}
        {expanded &&
          message.tool.changes.map((change) => (
            <button key={change.path} className="diff-link" onClick={() => onDiff(change)}>
              <FileDiff size={14} />
              {change.path}
            </button>
          ))}
      </details>
    );
  return (
    <article className={`message ${message.role} ${message.plan ? 'plan-card' : ''}`}>
      <div className="message-body">
        <MarkdownBody text={message.text} report={report} />
      </div>
      {message.plan && message.complete && (
        <div className="plan-actions">
          <small>{revisingPlan ? 'Continuing in Plan mode' : 'Proposed plan'}</small>
          {activePlan && !revisingPlan && (
            <div className="plan-choices" role="group" aria-label="Plan next steps">
              <button className="primary" disabled={!onPlan} onClick={() => onPlan?.(message, 'implement')}>
                Implement plan
              </button>
              <button
                disabled={!onPlan}
                title="Start a new session with this plan. This conversation is kept."
                onClick={() => onPlan?.(message, 'fresh')}
              >
                Clear context and implement
              </button>
              <button disabled={!onPlan} onClick={() => onPlan?.(message, 'revise')}>
                Continue planning
              </button>
            </div>
          )}
        </div>
      )}
      {message.role === 'user' && message.sentAt != null && (
        <time
          className="message-time"
          dateTime={new Date(message.sentAt * 1000).toISOString()}
          title={fullTime(message.sentAt)}
        >
          {clockTime(message.sentAt)}
        </time>
      )}
    </article>
  );
});

const MarkdownBody = memo(function MarkdownBody({ text, report }: { text: string; report: (error: unknown) => void }) {
  return (
    <Markdown
      remarkPlugins={[remarkGfm]}
      skipHtml
      components={{
        a: ({ href, children }) => (
          <a
            href={href}
            onClick={(event) => {
              event.preventDefault();
              if (href) void openLink(href).catch(report);
            }}
          >
            {children}
          </a>
        ),
        img: () => null,
      }}
    >
      {text}
    </Markdown>
  );
});
