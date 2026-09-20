import { relativePath } from './context';

export function linkKind(href: string): 'web' | 'file' | 'unsupported' {
  if (!href || /[\x00-\x20\\]/.test(href) || href.startsWith('#') || href.startsWith('//')) return 'unsupported';
  if (/^https?:\/\//i.test(href)) return 'web';
  if (/^tauri:/i.test(href)) {
    try {
      const url = new URL(href);
      return url.hostname === 'localhost' && !url.username && !url.password && !url.port ? 'file' : 'unsupported';
    } catch {
      return 'unsupported';
    }
  }
  return /^[a-z][a-z\d+.-]*:/i.test(href) ? 'unsupported' : 'file';
}

// Resolve only within the conversation's remote workspace, never the WebView's
// origin or this computer's filesystem. Decode once before checking traversal.
export function workspaceFilePath(href: string, workspace: string, base = workspace): string {
  if (linkKind(href) !== 'file') throw new Error('This is not a supported remote file link.');
  const encoded = /^tauri:/i.test(href) ? new URL(href).pathname : href.split(/[?#]/, 1)[0];
  let path: string;
  try {
    path = decodeURIComponent(encoded);
  } catch {
    throw new Error('The file link contains invalid URL encoding.');
  }
  if (!path || /[\x00-\x1f\x7f\\]/.test(path)) throw new Error('The file link contains an invalid path.');
  const parts: string[] = [];
  for (const part of (path.startsWith('/') ? path : `${base}/${path}`).split('/')) {
    if (!part || part === '.') continue;
    if (part === '..') parts.pop();
    else parts.push(part);
  }
  const relative = relativePath(workspace, `/${parts.join('/')}`);
  if (relative === undefined) throw new Error('This file is outside the current conversation workspace.');
  if (!relative) throw new Error('This link points to the workspace directory, not a file.');
  return relative;
}
