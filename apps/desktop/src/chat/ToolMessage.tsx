import { Brain, ChevronRight, FileDiff, Globe, Plug, Terminal, Wrench } from 'lucide-react';
import { memo, useId, useState } from 'react';
import { openLink } from '../bridge/client';
import type { ToolItem } from '../bridge/types';
import type { FileChange } from '../files/useFiles';
import { ToolOutput } from './ToolOutput';
import './ToolMessage.css';
import { RowMenu } from '../ui/RowMenu';

const kinds = {
  commandExecution: { icon: Terminal, title: 'Command' },
  fileChange: { icon: FileDiff, title: 'File changes' },
  webSearch: { icon: Globe, title: 'Search the web' },
  mcpToolCall: { icon: Plug, title: 'MCP tool' },
  dynamicToolCall: { icon: Wrench, title: 'Tool call' },
  reasoning: { icon: Brain, title: 'Reasoning summary' },
};
const statuses: Record<string, string> = {
  inProgress: 'Running',
  completed: 'Done',
  failed: 'Failed',
  declined: 'Declined',
  cancelled: 'Cancelled',
  canceled: 'Cancelled',
  interrupted: 'Interrupted',
  pending: 'Waiting',
};

export const ToolMessage = memo(function ToolMessage({
  tool,
  onDiff,
  onOpenFile,
  report,
}: {
  tool: ToolItem;
  onDiff: (change: FileChange, newWindow?: boolean) => void;
  onOpenFile?: (href: string) => Promise<void>;
  report: (error: unknown) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const id = useId();
  const kind = kinds[tool.kind as keyof typeof kinds] ?? { icon: Wrench, title: 'Tool activity' };
  const title = tool.title.trim() && tool.title !== tool.kind ? tool.title : kind.title;
  const Icon = kind.icon;
  const hasDetails = Boolean(
    tool.input?.trim() || tool.output.trim() || tool.output_source || tool.changes.length || tool.links?.length,
  );
  const heading = (
    <>
      <Icon size={14} aria-hidden="true" />
      <span className="tool-title" title={title}>
        {title}
      </span>
      {statuses[tool.status] && <small className="tool-status">{statuses[tool.status]}</small>}
      {hasDetails && <ChevronRight className="tool-chevron" size={13} aria-hidden="true" />}
    </>
  );
  return (
    <div
      className="tool"
      data-message-id={tool.id}
      data-tool-id={tool.id}
      data-kind={tool.kind}
      data-status={tool.status}
    >
      {hasDetails ? (
        <button
          type="button"
          className="tool-heading"
          aria-expanded={expanded}
          aria-controls={id}
          onClick={() => setExpanded((value) => !value)}
        >
          {heading}
        </button>
      ) : (
        <div className="tool-heading">{heading}</div>
      )}
      {hasDetails && expanded && (
        <div className="tool-details" id={id}>
          {tool.input && (
            <section>
              <h3>{tool.kind === 'commandExecution' ? 'Command' : tool.kind === 'webSearch' ? 'Query' : 'Input'}</h3>
              <pre>{tool.input}</pre>
            </section>
          )}
          {!!tool.links?.length && (
            <ul className="tool-links">
              {tool.links.map((link, index) => (
                <li key={`${link.url}:${index}`}>
                  <a
                    href={link.url}
                    title={link.url}
                    onClick={(event) => {
                      event.preventDefault();
                      void openLink(link.url).catch(report);
                    }}
                  >
                    {link.title || link.url}
                  </a>
                  <span className="tool-url">{link.url}</span>
                  {link.description && <p>{link.description}</p>}
                </li>
              ))}
            </ul>
          )}
          <ToolOutput
            key={
              tool.output_source
                ? JSON.stringify([tool.output_source.session, tool.output_source.turn, tool.output_source.item])
                : tool.id
            }
            tool={tool}
            report={report}
            onOpenFile={onOpenFile}
          />
          {tool.changes.map((change) => (
            <RowMenu
              key={change.path}
              items={[
                { label: 'View changes', action: () => onDiff(change) },
                { label: 'Open in New Window', action: () => onDiff(change, true) },
              ]}
            >
              <button className="diff-link" onClick={() => onDiff(change)}>
                <FileDiff size={14} />
                {change.path}
              </button>
            </RowMenu>
          ))}
        </div>
      )}
    </div>
  );
});
