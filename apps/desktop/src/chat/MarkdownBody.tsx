import { memo } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { openLink } from '../bridge/client';
import { linkKind } from '../files/links';
import { remarkMathDelimiters } from './math';
import 'katex/dist/katex.min.css';
import './MarkdownBody.css';

export const MarkdownBody = memo(function MarkdownBody({
  text,
  report,
  onOpenFile,
}: {
  text: string;
  report: (error: unknown) => void;
  onOpenFile?: (href: string) => Promise<void>;
}) {
  return (
    <Markdown
      remarkPlugins={[remarkGfm, remarkMath, remarkMathDelimiters]}
      rehypePlugins={[[rehypeKatex, { trust: false, maxExpand: 1000, maxSize: 10, errorColor: 'inherit' }]]}
      skipHtml
      urlTransform={(url) => (linkKind(url) === 'unsupported' ? '' : url)}
      components={{
        a: ({ href, children }) =>
          !href ? (
            <>{children}</>
          ) : (
            <a
              href={href}
              onClick={(event) => {
                event.preventDefault();
                if (linkKind(href) === 'web') void openLink(href).catch(report);
                else if (onOpenFile) void onOpenFile(href).catch(report);
                else report(new Error('Open this conversation in its workspace to view the file.'));
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
