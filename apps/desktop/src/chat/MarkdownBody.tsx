import { memo } from 'react';
import Markdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { openLink } from '../bridge/client';
import { linkKind } from '../files/links';
import { remarkHeadingIds } from './headings';
import { remarkMathDelimiters } from './math';
import 'katex/dist/katex.min.css';
import './MarkdownBody.css';

export const MarkdownBody = memo(function MarkdownBody({
  text,
  report,
  onOpenFile,
  components,
  headingIds = false,
}: {
  text: string;
  report: (error: unknown) => void;
  onOpenFile?: (href: string) => Promise<void>;
  components?: Components;
  headingIds?: boolean;
}) {
  return (
    <Markdown
      remarkPlugins={[remarkGfm, remarkMath, remarkMathDelimiters, ...(headingIds ? [remarkHeadingIds] : [])]}
      rehypePlugins={[[rehypeKatex, { trust: false, maxExpand: 1000, maxSize: 10, errorColor: 'inherit' }]]}
      skipHtml
      urlTransform={(url) => (url.startsWith('#') && headingIds ? url : linkKind(url) === 'unsupported' ? '' : url)}
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
        ...components,
      }}
    >
      {text}
    </Markdown>
  );
});
