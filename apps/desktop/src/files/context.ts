import type { FileContext } from '../bridge/files';

export function remotePath(root: string, path = '') {
  return `/${[...root.split('/'), ...path.split('/')].filter(Boolean).join('/')}`;
}
export function within(directory: string, path: string) {
  return directory === '/' || directory === path || path.startsWith(`${directory}/`);
}
export function overlaps(a: string, b: string) {
  return within(a, b) || within(b, a);
}
export function visibleIn(context: FileContext | null, file: { server: string; root: string; path: string }) {
  return Boolean(context && context.server === file.server && within(context.path, remotePath(file.root, file.path)));
}

export function fileUri(file: { server: string; root: string; path: string }) {
  const path = remotePath(file.root, file.path).split('/').map(encodeURIComponent).join('/');
  return `remote:///${encodeURIComponent(file.server)}${path}`;
}
