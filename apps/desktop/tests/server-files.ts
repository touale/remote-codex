import { $, $$, browser, expect } from '@wdio/globals';
import type { Catalog, Server, TextFile } from '../src/bridge/types';

import { heldFile, holdFiles, releaseFile, restoreFiles } from './file-gate';
import { activateTree, contextAt } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export async function emptyServerFiles(invoke: Invoke, server: Server, other: Server, remote: string) {
  const row = (s: Server) => `.server-row[data-server-id="${s.id}"] .tree-label`;
  const loading = '[aria-label="Loading files"]';
  const home = '[aria-label="Server home"]';
  await holdFiles(server.name);
  let request = -1;
  try {
    await activateTree(row(server));
    request = await heldFile('file_context_open', server.name);
    await expect($(home)).toHaveAttribute('data-server-id', server.id);
    await expect($(loading)).toBeDisplayed();
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/server-files-loading.png', import.meta.url).pathname,
    );
    await invoke('app_preferences', { patch: { theme: 'dark' } });
    await expect($('html')).toHaveAttribute('data-theme', 'dark');
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/server-files-loading-dark.png', import.meta.url).pathname,
    );
    await invoke('app_preferences', { patch: { theme: 'light' } });
    // A response arriving after leaving must neither navigate nor retain its file handle.
    await activateTree(row(other));
    await releaseFile(request);
    await browser.waitUntil(() => browser.execute((id) => window.fileGate.held[id].settled, request));
    await expect($(home)).toHaveAttribute('data-server-id', other.id);
    await expect($('.file-tree')).toHaveAttribute('data-server', other.name);
    await expect($('.workspace-path')).toHaveText('/');
    await $('.file-row .tree-label[title="tmp"]').waitForDisplayed();
    await activateTree(row(server));
    request = await heldFile('file_context_open', server.name);
    await releaseFile(request, true);
    await expect($('.file-tree [role="alert"]')).toHaveText('Fixture file operation interrupted.');
    await expect($(home)).toHaveAttribute('data-server-id', server.id);
    await $('.file-tree .tree-retry').click();
    request = await heldFile('file_context_open', server.name);
    await releaseFile(request);
    await $('.file-row .tree-label[title="tmp"]').waitForDisplayed();
  } finally {
    await restoreFiles();
  }
  const context = await browser.execute((id) => window.fileGate.held[id].context!.id, request);
  const workspaces = async () =>
    (await invoke<Catalog>('catalog', { archived: false })).workspaces.filter((w) => w.server_id === server.id);
  expect(await workspaces()).toHaveLength(0);
  await expect($('.workspace-path')).toHaveText('/');
  const directory = `${remote.slice(1)}/root-files`;
  await invoke('file_change', { context, change: { action: 'directory', path: directory } });
  await invoke('file_write', { context, path: `${directory}/child.txt`, text: 'Server files', revision: null });
  for (const path of ['tmp', remote.slice(1), directory]) {
    const label = $(`.file-row .tree-label[title="${path}"]`);
    await label.waitForDisplayed();
    await label.click();
  }
  await contextAt(`.file-row .tree-label[title="${directory}"]`);
  await expect($('[role="menuitem"]=Upload folder…')).toBeDisplayed();
  await expect($('[role="menuitem"]=Download…')).toBeDisplayed();
  await $('[role="menuitem"]=New folder…').click();
  await $('[role="dialog"] input').setValue('nested');
  await $('button=Create').click();
  await $(`.file-row .tree-label[title="${directory}/nested"]`).waitForDisplayed();
  await $(`.file-row .tree-label[title="${directory}/child.txt"]`).click();
  await $('.monaco-editor textarea').waitForExist();
  await $('.monaco-editor textarea').addValue(' edited');
  await $('button[aria-label="Save file (⌘S)"]').waitForEnabled();
  expect((await invoke<TextFile>('file_read', { context, path: `${directory}/child.txt` })).text).toBe('Server files');
  // Saving a workspace must not replace the current root browser.
  await invoke('workspace_open', { operationId: crypto.randomUUID(), server: server.name, path: `/${directory}` });
  await expect($('.workspace-path')).toHaveText('/');
  await $(`[data-workspace-path="/${directory}"] .tree-label`).waitForDisplayed();
  await activateTree(`[data-workspace-path="/${directory}"] .tree-label`);
  await $('.file-row .tree-label[title="child.txt"]').click();
  await expect($$('.editor-tab')).toBeElementsArrayOfSize(1);
  await expect($('button[aria-label="Save file (⌘S)"]')).toBeEnabled();
  await $('button[aria-label="Save file (⌘S)"]').click();
  await browser.waitUntil(async () =>
    (await invoke<TextFile>('file_read', { context, path: `${directory}/child.txt` })).text.includes('edited'),
  );
  await $('.editor-tab button[aria-label^="Close "]').click();
  await invoke('workspace_remove', { server: server.name, path: `/${directory}` });
  await $(`[data-workspace-path="/${directory}"]`).waitForExist({ reverse: true });
  await activateTree(row(server));
  await $('.file-row .tree-label[title="tmp"]').waitForDisplayed();
  await expect($('.workspace-path')).toHaveText('/');
  expect(await workspaces()).toHaveLength(0);
  await browser.saveScreenshot(
    new URL('../../../.artifacts/desktop-e2e/server-files-root.png', import.meta.url).pathname,
  );
}
