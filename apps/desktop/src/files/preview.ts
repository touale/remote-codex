import { call, operationId } from '../bridge/client';
import type { TextFile } from '../bridge/types';

export interface PreviewContent {
  preview: { format: 'pdf' | 'image'; mime: string; data: ArrayBuffer };
}
const images: Record<string, string> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  webp: 'image/webp',
  gif: 'image/gif',
  bmp: 'image/bmp',
  svg: 'image/svg+xml',
};
export function previewType(path: string) {
  const extension = /\.([^./]+)$/.exec(path)?.[1].toLowerCase() ?? '';
  if (extension === 'pdf') return { format: 'pdf' as const, mime: 'application/pdf' };
  return Object.hasOwn(images, extension) ? { format: 'image' as const, mime: images[extension] } : null;
}
export const isMarkdown = (path: string) => /\.(md|markdown|mdown)$/i.test(path);

export async function readPreview(context: string, path: string, signal: AbortSignal) {
  signal.throwIfAborted();
  const id = operationId();
  const cancel = () => {
    void call('cancel_operation', { id }).catch(() => {});
  };
  signal.addEventListener('abort', cancel, { once: true });
  try {
    const data = await call('file_preview_read', { context, path, operationId: id });
    signal.throwIfAborted();
    return data;
  } finally {
    signal.removeEventListener('abort', cancel);
  }
}
export async function readFile(context: string, path: string, signal: AbortSignal): Promise<TextFile | PreviewContent> {
  const type = previewType(path);
  if (!type) return call('file_read', { context, path });
  return { preview: { ...type, data: await readPreview(context, path, signal) } };
}
