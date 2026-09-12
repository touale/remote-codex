import { useRef } from 'react';
import { call } from '../bridge/client';
import type { FileContext } from '../bridge/files';
import type { Entry } from '../bridge/types';
import type { Ask } from '../ui/useDialog';
import { remotePath, validName, within } from './context';
import type { useFiles } from './useFiles';

export function useFileActions(
  context: FileContext | null,
  files: Pick<ReturnType<typeof useFiles>, 'get' | 'save' | 'close' | 'open' | 'allTabs' | 'move'>,
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
    if (buffer?.transferring) throw new Error('Wait for the file to finish moving to its new window.');
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
    if (!validName(name)) throw { message: 'Enter a single name without slashes or control characters.' };
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
      remove &&
      files
        .allTabs()
        .some(
          (buffer) =>
            buffer.server === current.server &&
            within(remotePath(current.path, entry.path), remotePath(buffer.root, buffer.path)),
        )
    ) {
      throw { message: 'Close the affected editor tabs before deleting this entry.' };
    }
    const answer = await ask(
      remove
        ? {
            title: `Delete ${entry.name}?`,
            message: 'This removes the remote entry. Directories must be empty.',
            choices: ['Delete'],
          }
        : { title: 'Rename', input: { label: 'Name', value: entry.name }, choices: ['Rename'] },
    );
    if (!answer) return;
    if (remove) await call('file_change', { context: current.id, change: { action: 'remove', path: entry.path } });
    else {
      if (!validName(answer)) throw { message: 'Enter a single name without slashes or control characters.' };
      const parent = entry.path.split('/').slice(0, -1);
      const destination = [...parent, answer].join('/');
      if (destination !== entry.path) await files.move(current, entry.path, destination);
    }
    if (remove) refreshFiles();
  };
  const move = async (entry: Entry, parent: string) => {
    if (!context) return;
    const destination = parent ? `${parent}/${entry.name}` : entry.name;
    if (destination === entry.path) return;
    if (entry.directory && within(entry.path, parent)) throw { message: 'A folder cannot be moved into itself.' };
    await files.move(context, entry.path, destination);
  };
  return { save, close, create, change, move };
}
