import { useEffect, useState, type ReactNode } from 'react';
import type { PreviewView } from '../bridge/editor';
import { EditorSkeleton } from './FileLoading';
import { ErrorText } from '../ui/controls';
import type { PreviewBuffer } from './tabs';

export default function ImagePreview({
  file,
  onView,
  actions,
}: {
  file: PreviewBuffer;
  actions: ReactNode;
  onView: (view: PreviewView) => void;
}) {
  const [url, setUrl] = useState('');
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [error, setError] = useState('');
  const scale = file.previewView?.scale ?? 'fit';
  useEffect(() => {
    const url = URL.createObjectURL(new Blob([file.preview.data], { type: file.preview.mime }));
    setUrl(url);
    setError('');
    return () => URL.revokeObjectURL(url);
  }, [file.preview]);
  return (
    <div className="file-preview image-preview">
      <div className="preview-toolbar" aria-label="Image controls">
        <button onClick={() => onView({ scale: 'fit' })} aria-pressed={scale === 'fit'}>
          Fit
        </button>
        <button onClick={() => onView({ scale: 1 })} aria-pressed={scale === 1}>
          100%
        </button>
        <button
          aria-label="Zoom out"
          onClick={() => onView({ scale: Math.max(0.1, (typeof scale === 'number' ? scale : 1) / 1.25) })}
        >
          −
        </button>
        <button
          aria-label="Zoom in"
          onClick={() => onView({ scale: Math.min(10, (typeof scale === 'number' ? scale : 1) * 1.25) })}
        >
          +
        </button>
        {typeof scale === 'number' && <span>{Math.round(scale * 100)}%</span>}
        <span className="spacer" />
        {size.width > 0 && (
          <span>
            {size.width} × {size.height}
          </span>
        )}
        {actions}
      </div>
      <div className="image-viewport">
        {!size.width && !error && (
          <div className="preview-loading">
            <EditorSkeleton />
          </div>
        )}
        {error ? (
          <ErrorText message={error} />
        ) : (
          url && (
            <img
              src={url}
              alt={file.path.split('/').at(-1)}
              draggable={false}
              className={scale === 'fit' ? 'fit' : ''}
              style={
                typeof scale === 'number' && size.width
                  ? { width: size.width * scale, height: size.height * scale }
                  : undefined
              }
              onLoad={(event) =>
                setSize({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight })
              }
              onError={() => setError('This image could not be decoded. Download it to open it locally.')}
            />
          )
        )}
      </div>
    </div>
  );
}
