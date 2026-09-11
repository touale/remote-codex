import { $, browser, expect } from '@wdio/globals';
import { heldFile, holdFiles, releaseFile, restoreFiles } from './file-gate';
import { activateTree } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
const entry = (path: string) => `.file-row .tree-label[title="${path}"]`;
const selected = '.editor-tab.selected .tab-name';
const skeleton = '[aria-label="Loading file content"]';
const save = 'button[aria-label="Save file (⌘S)"]';
const close = (name: string) => $(`button[aria-label="Close ${name}"]`).click();
const screenshot = (name: string) =>
  browser.saveScreenshot(new URL(`../../../.artifacts/desktop-e2e/${name}.png`, import.meta.url).pathname);

export async function directoryLoading(invoke: Invoke, workspace: string, remote: string) {
  for (const path of ['loading', 'loading/branch', 'loading/retry-folder'])
    await invoke('file_change', { context: workspace, change: { action: 'directory', path } });
  for (const path of ['a.txt', 'b.txt', 'branch/child.txt', 'retry-folder/child.txt'])
    await invoke('file_write', {
      context: workspace,
      path: `loading/${path}`,
      text: `Content of ${path}`,
      revision: null,
    });
  await invoke('workspace_open', {
    operationId: crypto.randomUUID(),
    server: 'desktop-test',
    path: `${remote}/loading`,
  });
  await $(`[data-workspace-path="${remote}/loading"] .tree-label`).waitForDisplayed();
  await invoke('app_preferences', { patch: { theme: 'light' } });
  await expect($('html')).toHaveAttribute('data-theme', 'light');
  await holdFiles();
  try {
    await activateTree(`[data-workspace-path="${remote}/loading"] .tree-label`);
    let request = await heldFile('file_list', '');
    await expect($('[aria-label="Loading files"]')).toBeDisplayed();
    await expect($('.file-tree')).not.toHaveText(expect.stringContaining('Choose a'));
    await releaseFile(request);
    await $(entry('a.txt')).waitForDisplayed();
    await $(entry('branch')).click();
    request = await heldFile('file_list', 'branch');
    await expect($('[aria-label="Loading branch"]')).toBeDisplayed();
    await screenshot('directory-loading-light');
    await invoke('app_preferences', { patch: { theme: 'dark' } });
    await expect($('html')).toHaveAttribute('data-theme', 'dark');
    await screenshot('directory-loading-dark');
    await invoke('app_preferences', { patch: { theme: 'light' } });
    await $(entry('branch')).click();
    await releaseFile(request);
    await expect($(entry('branch'))).toHaveAttribute('aria-expanded', 'false');
    await expect($(entry('branch/child.txt'))).not.toExist();
    await $(entry('branch')).click();
    await $(entry('branch/child.txt')).waitForDisplayed();
    await expect($('[aria-label="Loading branch"]')).not.toExist();
    await $(entry('retry-folder')).click();
    request = await heldFile('file_list', 'retry-folder');
    await releaseFile(request, true);
    // The Retry belongs to this directory and must not reload the root.
    await expect($('.tree-load-error [role="alert"]')).toHaveText('Fixture file operation interrupted.');
    await $('.tree-load-error .tree-retry').click();
    await releaseFile(await heldFile('file_list', 'retry-folder', request));
    await $(entry('retry-folder/child.txt')).waitForDisplayed();
    await expect($('.tree-load-error')).not.toExist();
    // Newer refresh results win; cached entries remain visible throughout.
    await $('button[aria-label="Refresh files"]').click();
    const old = await heldFile('file_list', '');
    await expect($(entry('a.txt'))).toBeDisplayed();
    await expect($('[aria-label="Loading files"]')).not.toExist();
    await invoke('file_write', { context: workspace, path: 'loading/new.txt', text: 'new', revision: null });
    await $('button[aria-label="Refresh files"]').click();
    const latest = await heldFile('file_list', '', old);
    await releaseFile(latest);
    await $(entry('new.txt')).waitForDisplayed();
    await releaseFile(old);
    await expect($(entry('new.txt'))).toBeDisplayed();
  } finally {
    await restoreFiles();
  }
}

export async function editorLoading(invoke: Invoke, remote: string) {
  await holdFiles();
  try {
    await $(entry('a.txt')).click();
    const a = await heldFile('file_read', 'a.txt');
    await expect($(selected)).toHaveText('a.txt');
    await expect($(skeleton)).toBeDisplayed();
    await expect($(save)).toBeDisabled();
    await expect($('.monaco-editor')).not.toExist();
    await screenshot('editor-loading-light');
    await invoke('app_preferences', { patch: { theme: 'dark' } });
    await expect($('html')).toHaveAttribute('data-theme', 'dark');
    await screenshot('editor-loading-dark');
    await invoke('app_preferences', { patch: { theme: 'light' } });
    await $(entry('b.txt')).click();
    const b = await heldFile('file_read', 'b.txt');
    await releaseFile(a);
    await expect($(selected)).toHaveText('b.txt');
    await expect($(skeleton)).toBeDisplayed();
    await releaseFile(b);
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/Content\s+of\s+b\.txt/));
    const reads = await browser.execute(() => window.fileGate.held.length);
    await $(entry('a.txt')).click();
    await expect($(selected)).toHaveText('a.txt');
    await expect($(skeleton)).not.toExist();
    expect(await browser.execute(() => window.fileGate.held.length)).toBe(reads);
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/Content\s+of\s+a\.txt/));
    await close('b.txt');
    await $(entry('b.txt')).click();
    const failed = await heldFile('file_read', 'b.txt', b);
    await releaseFile(failed, true);
    await expect($('.editor-pane [role="alert"]')).toHaveText('Fixture file operation interrupted.');
    await expect($(save)).toBeDisabled();
    await $('.editor-pane').$('button=Retry').click();
    const retry = await heldFile('file_read', 'b.txt', failed);
    await expect($(skeleton)).toBeDisplayed();
    await $(entry('a.txt')).click();
    await releaseFile(retry);
    await expect($(selected)).toHaveText('a.txt');
    await close('b.txt');
    await close('a.txt');
    await $(entry('a.txt')).click();
    const pending = await heldFile('file_read', 'a.txt', a);
    await close('a.txt');
    await releaseFile(pending);
    await expect($('.editor-tab')).not.toExist();
    await expect($('.editor-pane')).not.toExist();
  } finally {
    await restoreFiles();
  }
  await activateTree(`[data-workspace-path="${remote}"] .tree-label`);
  await expect($('.workspace-path')).toHaveText(remote);
  await invoke('workspace_remove', { server: 'desktop-test', path: `${remote}/loading` });
  await $(`[data-workspace-path="${remote}/loading"]`).waitForExist({ reverse: true });
}
