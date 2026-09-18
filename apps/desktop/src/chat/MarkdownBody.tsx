import { memo } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { openLink } from '../bridge/client';
import { remarkMathDelimiters } from './math';
import 'katex/dist/katex.min.css';
import './MarkdownBody.css';

export const MarkdownBody = memo(function MarkdownBody({
  text,
  report,
}: {
  text: string;
  report: (error: unknown) => void;
}) {
  return (
    <Markdown
      remarkPlugins={[remarkGfm, remarkMath, remarkMathDelimiters]}
      rehypePlugins={[[rehypeKatex, { trust: false, maxExpand: 1000, maxSize: 10, errorColor: 'inherit' }]]}
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
