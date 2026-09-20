import { useEffect, useRef, useState, type ReactNode } from 'react';
import { AnnotationMode, getDocument, GlobalWorkerOptions } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { EventBus, PDFLinkService, PDFViewer } from 'pdfjs-dist/legacy/web/pdf_viewer.mjs';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import 'pdfjs-dist/legacy/web/pdf_viewer.css';
import { failure, openLink } from '../bridge/client';
import type { PreviewView } from '../bridge/editor';
import { ErrorText } from '../ui/controls';
import { EditorSkeleton } from './FileLoading';
import type { PreviewBuffer } from './tabs';

GlobalWorkerOptions.workerSrc = workerUrl;
export default function PdfPreview({
  file,
  onView,
  actions,
}: {
  file: PreviewBuffer;
  actions: ReactNode;
  onView: (view: PreviewView) => void;
}) {
  const container = useRef<HTMLDivElement>(null);
  const viewer = useRef<PDFViewer | null>(null);
  const changed = useRef(onView);
  changed.current = onView;
  const [error, setError] = useState('');
  const [loaded, setLoaded] = useState(false);
  const [pages, setPages] = useState(0);
  const [page, setPage] = useState(file.previewView?.page ?? 1);
  const [scale, setScale] = useState(1);
  const changeScale = (scale: number | 'page-width') => {
    const current = viewer.current;
    if (!current?.pdfDocument) return;
    const page = current.currentPageNumber;
    current.currentScaleValue = String(scale);
    current.currentPageNumber = page;
  };
  useEffect(() => {
    const element = container.current!;
    const lifetime = new AbortController();
    const eventBus = new EventBus();
    const linkService = new PDFLinkService({ eventBus });
    const options = {
      container: element,
      eventBus,
      linkService,
      abortSignal: lifetime.signal,
      annotationMode: AnnotationMode.ENABLE,
      maxCanvasPixels: 16 * 1024 * 1024,
    };
    let pdfViewer: PDFViewer;
    try {
      pdfViewer = new PDFViewer(options);
    } catch (error) {
      lifetime.abort();
      setError(failure(error).message);
      return;
    }
    linkService.setViewer(pdfViewer);
    viewer.current = pdfViewer;
    const initial = file.previewView;
    eventBus.on('pagesinit', () => {
      pdfViewer.currentScaleValue =
        initial?.scale === undefined || initial.scale === 'fit' ? 'page-width' : String(initial.scale);
      pdfViewer.currentPageNumber = Math.min(initial?.page ?? 1, pdfViewer.pagesCount);
      setPages(pdfViewer.pagesCount);
    });
    eventBus.on('pagerendered', ({ error }: { error?: unknown }) => {
      if (lifetime.signal.aborted) return;
      if (error) setError(failure(error).message);
      setLoaded(true);
    });
    const saveView = () => {
      if (lifetime.signal.aborted) return;
      setPage(pdfViewer.currentPageNumber);
      setScale(pdfViewer.currentScale);
      changed.current({
        page: pdfViewer.currentPageNumber,
        scale: pdfViewer.currentScaleValue === 'page-width' ? 'fit' : pdfViewer.currentScale,
      });
    };
    eventBus.on('pagechanging', saveView);
    eventBus.on('scalechanging', saveView);
    setLoaded(false);
    setError('');
    // PDF.js transfers ownership to its worker. Keep the tab's bytes intact.
    const loading = getDocument({
      data: new Uint8Array(file.preview.data.slice(0)),
      cMapUrl: '/pdf-assets/cmaps/',
      cMapPacked: true,
      standardFontDataUrl: '/pdf-assets/standard_fonts/',
      wasmUrl: '/pdf-assets/wasm/',
      enableXfa: false,
    });
    let passwordRequired = false;
    loading.onPassword = () => {
      passwordRequired = true;
      if (!lifetime.signal.aborted) setError('This PDF is password protected. Download it to open it locally.');
      void loading.destroy();
    };
    void loading.promise
      .then((document) => {
        if (lifetime.signal.aborted) return;
        linkService.setDocument(document);
        pdfViewer.setDocument(document);
      })
      .catch((error) => {
        if (!lifetime.signal.aborted && !passwordRequired) setError(failure(error).message);
      });
    const resize = new ResizeObserver(() => {
      if (pdfViewer.pdfDocument && pdfViewer.currentScaleValue === 'page-width') changeScale('page-width');
    });
    resize.observe(element);
    return () => {
      lifetime.abort();
      resize.disconnect();
      viewer.current = null;
      pdfViewer.cleanup();
      // The upstream signature omits the supported null reset argument.
      pdfViewer.setDocument(null as unknown as Parameters<PDFViewer['setDocument']>[0]);
      void loading.destroy();
    };
  }, [file.preview]);
  const zoom = (factor: number) => {
    changeScale(Math.min(10, Math.max(0.1, scale * factor)));
  };
  return (
    <div className="file-preview pdf-preview">
      <div className="preview-toolbar" aria-label="PDF controls">
        <button
          disabled={page <= 1 || !loaded}
          aria-label="Previous page"
          onClick={() => {
            if (viewer.current) viewer.current.currentPageNumber = page - 1;
          }}
        >
          ‹
        </button>
        <input
          aria-label="Page number"
          type="number"
          min={1}
          max={pages || 1}
          value={page}
          disabled={!loaded}
          onChange={(e) => {
            const n = Number(e.target.value);
            if (viewer.current && n >= 1 && n <= pages) viewer.current.currentPageNumber = n;
          }}
        />
        <span>/ {pages || '—'}</span>
        <button
          disabled={page >= pages || !loaded}
          aria-label="Next page"
          onClick={() => {
            if (viewer.current) viewer.current.currentPageNumber = page + 1;
          }}
        >
          ›
        </button>
        <span className="spacer" />
        <button disabled={!loaded} aria-label="Zoom out" onClick={() => zoom(1 / 1.25)}>
          −
        </button>
        <span>{Math.round(scale * 100)}%</span>
        <button disabled={!loaded} aria-label="Zoom in" onClick={() => zoom(1.25)}>
          +
        </button>
        <button
          disabled={!loaded}
          onClick={() => {
            changeScale('page-width');
          }}
        >
          Fit width
        </button>
        {actions}
      </div>
      <div className="pdf-body">
        {!loaded && !error && (
          <div className="preview-loading">
            <EditorSkeleton />
          </div>
        )}
        {error && (
          <div className="preview-error">
            <ErrorText message={error} />
          </div>
        )}
        <div
          className="pdf-viewport"
          ref={container}
          onClick={(event) => {
            const anchor = (event.target as Element).closest('a');
            if (anchor) event.preventDefault();
            if (anchor && /^https?:/i.test(anchor.href)) {
              void openLink(anchor.href).catch((error) => setError(failure(error).message));
            }
          }}
        >
          <div className="pdfViewer" />
        </div>
      </div>
    </div>
  );
}
