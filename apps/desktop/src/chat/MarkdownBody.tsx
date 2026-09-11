import { memo } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { openLink } from '../bridge/client';

export const MarkdownBody = memo(function MarkdownBody({
  text,
  report,
}: {
  text: string;
  report: (error: unknown) => void;
}) {
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
