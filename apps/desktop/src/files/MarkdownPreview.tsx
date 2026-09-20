import { createContext, useContext, useDeferredValue, useEffect, useRef, useState } from 'react';
import type { Components } from 'react-markdown';
import { failure, openLink } from '../bridge/client';
import { MarkdownBody } from '../chat/MarkdownBody';
import { remotePath } from './context';
import { linkKind, workspaceFilePath } from './links';
import { previewType, readPreview } from './preview';
import type { Buffer } from './tabs';

function RemoteImage({ context, path, alt }: { context: string; path: string; alt?: string }) {
  const element = useRef<HTMLSpanElement>(null);
  const [url, setUrl] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    const controller = new AbortController();
    let url = '';
    setUrl('');
    setError('');
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.disconnect();
        const type = previewType(path);
        if (type?.format !== 'image') {
          setError('Unsupported image format');
          return;
        }
        void readPreview(context, path, controller.signal)
          .then((data) => {
            if (controller.signal.aborted) return;
            url = URL.createObjectURL(new Blob([data], { type: type.mime }));
            setUrl(url);
          })
          .catch((error) => {
            if (!controller.signal.aborted) setError(failure(error).message);
          });
      },
      { rootMargin: '200px' },
    );
    observer.observe(element.current!);
    return () => {
      observer.disconnect();
      controller.abort();
      if (url) URL.revokeObjectURL(url);
    };
  }, [context, path]);
  return (
    <span ref={element}>
      {error ? (
        <span className="preview-image-error" title={error}>
          {alt || 'Image unavailable'}
        </span>
      ) : url ? (
        <img src={url} alt={alt ?? ''} onError={() => setError('Could not decode image')} />
      ) : (
        <span className="preview-image-error" role="status">
          Loading image…
        </span>
      )}
    </span>
  );
}
interface DocumentContext {
  file: Buffer;
  base: string;
  container: React.RefObject<HTMLDivElement | null>;
  onOpenFile: (path: string) => Promise<void>;
  report: (error: unknown) => void;
}
const Document = createContext<DocumentContext | null>(null);
const components: Components = {
  a: function FileLink({ href, children }) {
    const { file, base, container, onOpenFile, report } = useContext(Document)!;
    return (
      <a
        href={href}
        onClick={(event) => {
          event.preventDefault();
          if (!href) return;
          try {
            if (href.startsWith('#')) {
              const id = decodeURIComponent(href.slice(1));
              container.current
                ?.querySelector<HTMLElement>(`[id="${CSS.escape(id)}"]`)
                ?.scrollIntoView({ block: 'start' });
            } else if (linkKind(href) === 'web') void openLink(href).catch(report);
            else void onOpenFile(workspaceFilePath(href, file.root, base)).catch(report);
          } catch (error) {
            report(error);
          }
        }}
      >
        {children}
      </a>
    );
  },
  img: function FileImage({ src, alt }) {
    const { file, base, report } = useContext(Document)!;
    if (!src) return null;
    if (linkKind(src) === 'web')
      return (
        <a
          href={src}
          onClick={(event) => {
            event.preventDefault();
            void openLink(src).catch(report);
          }}
        >
          {alt || 'External image'}
        </a>
      );
    try {
      return <RemoteImage context={file.context} path={workspaceFilePath(src, file.root, base)} alt={alt} />;
    } catch (error) {
      return (
        <span className="preview-image-error" title={failure(error).message}>
          {alt || 'Image unavailable'}
        </span>
      );
    }
  },
};
export function MarkdownPreview({
  file,
  onOpenFile,
  report,
}: {
  file: Buffer;
  onOpenFile: (path: string) => Promise<void>;
  report: (error: unknown) => void;
}) {
  const text = useDeferredValue(file.text);
  const container = useRef<HTMLDivElement>(null);
  const base = remotePath(file.root, file.path.split('/').slice(0, -1).join('/'));
  return (
    <Document.Provider value={{ file, base, container, onOpenFile, report }}>
      <div ref={container} className="markdown-rendered message-body" aria-label="Markdown preview">
        <MarkdownBody headingIds text={text} report={report} components={components} />
      </div>
    </Document.Provider>
  );
}
