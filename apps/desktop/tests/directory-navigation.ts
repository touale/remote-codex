import { $, browser, expect } from '@wdio/globals';
import type { Catalog, TextFile } from '../src/bridge/types';
import { activateTree } from './interaction-controls';
import { heldFile, holdFiles, releaseFile, restoreFiles } from './file-gate';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function directoryNavigation(invoke: Invoke, workspace: string, remote: string) {
  const parent = `${remote}/projects`;
  const selector = `[data-directory-path="${parent}"] .tree-label`;
  const before = (await invoke<Catalog>('catalog', { archived: false })).workspaces.map((w) => w.path).sort();
  await $(selector).click();
  await expect($('.workspace-path')).toHaveText(remote);
  await holdFiles('desktop-test');
  try {
    await activateTree(selector);
    const held = await heldFile('file_context_open', parent);
    await expect($('[aria-label="Start working"]')).toHaveText(
      expect.stringContaining('What would you like to work on?'),
    );
    await expect($('[aria-label="Start working"]')).toHaveText(expect.stringContaining(parent));
    await expect($(selector)).toHaveAttribute('aria-selected', 'true');
    await $('.file-tree [aria-label="Loading files"]').waitForExist();
    await releaseFile(held);
  } finally {
    await restoreFiles();
  }
  await $('.file-row .tree-label[title="test1"]').waitForDisplayed();
  await $('button[aria-label="Toggle terminal (⌘`)"]').click();
  await $('.xterm-screen').waitForDisplayed();
  await $('.xterm-helper-textarea').waitForExist();
  await expect($('.terminal-tab.selected > button')).toHaveText(parent);
  await $('.xterm-helper-textarea').addValue('pwd > directory-pwd.txt\r');
  await browser.waitUntil(async () => {
    try {
      return (
        (
          await invoke<TextFile>('file_read', { context: workspace, path: 'projects/directory-pwd.txt' })
        ).text.trim() === parent
      );
    } catch {
      return false;
    }
  });
  await $('button[aria-label="Close terminal"]').click();
  expect((await invoke<Catalog>('catalog', { archived: false })).workspaces.map((w) => w.path).sort()).toEqual(before);
  await $('[aria-label="Start working"] button[aria-label="New session"]').click();
  await $('textarea[aria-label="Message Codex"]').waitForDisplayed();
  await $(`[data-workspace-path="${parent}"]`).waitForDisplayed();
  await activateTree(`[data-workspace-path="${remote}"] .tree-label`);
  await expect($('.workspace-path')).toHaveText(remote);
}
