import { useEffect, useRef } from 'react';
import { attach, call } from '../bridge/client';
import type { Choice, FileContext, Granted, Transfer } from '../bridge/files';
import { remotePath, within } from '../files/context';
import type { useFiles } from '../files/useFiles';
import type { Ask } from '../ui/useDialog';
import { accept, transferStore } from './store';

export function useTransfers(
  files: Pick<ReturnType<typeof useFiles>, 'all' | 'save' | 'allTabs' | 'close'>,
  ask: Ask,
  report: (error: unknown) => void,
) {
  const refs = useRef({ files, ask });
  refs.current = { files, ask };
  useEffect(() => {
    void attach().then(transferStore.refresh).catch(report);
  }, [report]);
  const guard = async (server: string, root: string, directory: string) => {
    const { files, ask } = refs.current;
    const affected = () =>
      files.all().filter((b) => b.server === server && within(remotePath(root, directory), remotePath(b.root, b.path)));
    const dirty = affected().filter((b) => b.text !== b.original);
    if (dirty.length) {
      const answer = await ask({
        title: 'Save changes before uploading?',
        message: `${dirty.length} open file(s) are inside the upload destination.`,
        choices: ['Save', 'Discard'],
      });
      if (!answer) return false;
      if (answer === 'Save') {
        for (const buffer of affected().filter((b) => b.text !== b.original)) await files.save(buffer.key);
        if (affected().some((b) => b.text !== b.original))
          throw { message: 'New edits arrived while saving. Review them before uploading.' };
      }
    }
    for (const tab of files
      .allTabs()
      .filter((b) => b.server === server && within(remotePath(root, directory), remotePath(b.root, b.path))))
      files.close(tab.key);
    return true;
  };
  const run = (task: Transfer) => {
    accept({ ...task, active: true, owned: true, status: 'preparing' });
    void call('transfer_run', { id: task.id }).catch(() => transferStore.refresh().catch(report));
  };
  const upload = async (workspace: FileContext, destination: string, grant: Granted) => {
    if (!(await guard(workspace.server, workspace.path, destination))) {
      await call('transfer_discard_grant', { token: grant.token });
      return;
    }
    const task = await call('transfer_upload', { context: workspace.id, destination, token: grant.token });
    run(task);
  };
  return {
    upload,
    pick: async (workspace: FileContext, destination: string, folder: boolean) => {
      const grant = await call('transfer_pick', { folder });
      if (grant) await upload(workspace, destination, grant);
    },
    download: async (workspace: FileContext, source: string) => {
      const task = await call('transfer_download', { context: workspace.id, source });
      if (task) run(task);
    },
    resume: async (task: Transfer, choice?: Choice, all = false, restart = false) => {
      if (task.direction === 'upload' && !(await guard(task.server, task.workspace, task.destination))) return;
      if (choice) await call('transfer_action', { id: task.id, action: { type: 'resolve', choice, all } });
      if (restart) await call('transfer_action', { id: task.id, action: { type: 'restart' } });
      run(task);
    },
    pause: async (task: Transfer) => {
      await call('transfer_action', { id: task.id, action: { type: 'pause' } });
      await transferStore.refresh();
    },
    cancel: async (task: Transfer) => {
      await call('transfer_action', { id: task.id, action: { type: 'cancel' } });
      run(task);
    },
  };
}

export type TransferActions = ReturnType<typeof useTransfers>;
