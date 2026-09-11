import { useRef } from 'react';
import { call } from '../bridge/client';
import type { FileContext } from '../bridge/files';
import type { Entry } from '../bridge/types';
import type { Ask } from '../ui/useDialog';
import { remotePath, within } from './context';
import type { useFiles } from './useFiles';

export function useFileActions(
  context: FileContext | null,
  files: Pick<ReturnType<typeof useFiles>, 'get' | 'save' | 'close' | 'open' | 'allTabs'>,
  ask: Ask,
  showEditor: () => void,
  refreshFiles: () => void,
) {
  const activeContext = useRef(context?.id);
  activeContext.current = context?.id;
  const save = async (key: string, resolve = false) => {
    await files.save(key, resolve);
    refreshFiles();
  };
  const close = async (key: string) => {
    const buffer = files.get(key);
    if (buffer && buffer.text !== buffer.original) {
      const answer = await ask({
        title: 'Save changes?',
        message: `${buffer.server} · ${buffer.root}/${buffer.path}`,
        choices: ['Save', 'Discard'],
      });
      if (!answer) return false;
      if (answer === 'Save') {
        await save(key);
        const latest = files.get(key);
        if (latest && latest.text !== latest.original)
          throw { message: 'New edits arrived while saving. Save or discard them before closing.' };
      }
    }
    files.close(key);
    return true;
  };
  const create = async (directory: boolean, parent: string) => {
    if (!context) return false;
    const current = context;
    const name = await ask({
      title: directory ? 'New folder' : 'New file',
      message: `${current.server} · ${current.path.replace(/\/$/, '')}/${parent}`,
      input: { label: directory ? 'Folder name' : 'File name' },
      choices: ['Create'],
    });
    if (!name) return false;
    if (!name.trim() || name === '.' || name === '..' || /[/\\\x00-\x1f]/.test(name))
      throw { message: 'Enter a single name without slashes or control characters.' };
    const path = parent ? `${parent}/${name}` : name;
    if (directory) await call('file_change', { context: current.id, change: { action: 'directory', path } });
    else {
      await call('file_write', { context: current.id, path, text: '', revision: null });
      if (activeContext.current === current.id) {
        showEditor();
        await files.open(current.id, path, current.server, current.path);
      }
    }
    return true;
  };
  const change = async (entry: Entry, remove: boolean) => {
    if (!context) return;
    const current = context;
    if (
      files
        .allTabs()
        .some(
          (buffer) =>
            buffer.server === current.server &&
            within(remotePath(current.path, entry.path), remotePath(buffer.root, buffer.path)),
        )
    ) {
      throw { message: 'Close the affected editor tabs before renaming or deleting this entry.' };
    }
    const answer = await ask(
      remove
        ? {
            title: `Delete ${entry.name}?`,
            message: 'This removes the remote entry. Directories must be empty.',
            choices: ['Delete'],
          }
        : { title: 'Rename', input: { label: 'New relative path', value: entry.path }, choices: ['Rename'] },
    );
    if (!answer) return;
    await call('file_change', {
      context: current.id,
      change: remove
        ? { action: 'remove', path: entry.path }
        : { action: 'rename', path: entry.path, destination: answer },
    });
    refreshFiles();
  };
  return { save, close, create, change };
}
