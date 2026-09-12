import { $, browser, expect } from '@wdio/globals';
import { withExecuteOptions } from '@wdio/tauri-service';
import type { TextFile } from '../src/bridge/types';
import { invokeWindow } from './invoke';
import { contextAt } from './interaction-controls';

export async function diffContentWindow(setTheme: (theme: 'light' | 'dark') => Promise<unknown>) {
  const main = await browser.getWindowHandle();
  const previous = await browser.getWindowHandles();
  await contextAt('[data-tool-id="patch"] .diff-link');
  await $('[role="menuitem"]=Open in New Window').click();
  await browser.waitUntil(async () => (await browser.getWindowHandles()).length > previous.length);
  const target = (await browser.getWindowHandles()).find((label) => !previous.includes(label))!;
  try {
    await browser.switchToWindow(target);
    await expect($('.content-window .change-scroll')).toHaveAttribute('data-layout', 'split');
    await expect($('.change-row code.deletion')).toHaveText('old');
    await expect($('.change-row code.addition')).toHaveText('new');
    await $('button=Unified').click();
    await expect($('.change-scroll')).toHaveAttribute('data-layout', 'unified');
    await expect($('.change-row code.deletion')).toHaveText(expect.stringContaining('old'));
    await $('button=Split').click();
    for (const theme of ['light', 'dark'] as const) {
      await setTheme(theme);
      await expect($('html')).toHaveAttribute('data-theme', theme);
      await browser.saveScreenshot(
        new URL(`../../../.artifacts/desktop-e2e/content-diff-${theme}.png`, import.meta.url).pathname,
      );
    }
  } finally {
    await closeContentTestWindow(target);
    await browser.switchToWindow(main);
  }
}

export async function contentWindows(remote: string) {
  const main = await browser.getWindowHandle();
  const source = await invokeWindow<string>('main', 'new_window', {
    target: { kind: 'workspace', server: 'desktop-test', path: remote },
  });
  await browser.switchToWindow(source);
  await $('.file-tree').waitForDisplayed();
  const workspace = await invokeWindow<{ id: string }>('main', 'workspace_open', {
    operationId: crypto.randomUUID(),
    server: 'desktop-test',
    path: remote,
  });
  const file = { context: workspace.id, path: 'window-test.txt' };
  await invokeWindow('main', 'file_write', { ...file, revision: null, text: 'original\n' });
  await $('button[aria-label="Refresh files"]').waitForEnabled();
  await $('button[aria-label="Refresh files"]').click();
  await $('.file-row .tree-label[title="window-test.txt"]').waitForDisplayed();
  await contextAt('.file-row .tree-label[title="window-test.txt"]');
  await $('[role="menuitem"]=Open in New Window').click();
  await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 3);
  const preview = (await browser.getWindowHandles()).find((label) => label !== source && label !== main)!;
  await browser.switchToWindow(preview);
  await $('.content-window .monaco-editor').waitForDisplayed({ timeout: 60000 });
  await expect($('.view-lines')).toHaveText(expect.stringContaining('original'));
  await closeContentTestWindow(preview);
  await browser.switchToWindow(source);
  await expect($('.editor-pane')).not.toExist();
  await $('.file-row .tree-label[title="window-test.txt"]').click();
  await $('.monaco-editor textarea').waitForExist();
  await $('.monaco-editor textarea').addValue('unsaved transfer\n');
  await expect($('.editor-status')).toHaveText(expect.stringContaining('Unsaved changes'));
  await $('button[aria-label="Move to New Window"]').click();
  await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 3);
  const destination = (await browser.getWindowHandles()).find((label) => label !== source && label !== main)!;
  try {
    await browser.switchToWindow(destination);
    await $('.content-window .monaco-editor').waitForDisplayed({ timeout: 60000 });
    await expect($('.view-lines')).toHaveText(expect.stringMatching(/unsaved\s+transfer/));
    await expect($('.editor-status')).toHaveText(expect.stringContaining('Unsaved changes'));
    await expect($('aside')).not.toExist();
    await expect($('.chat')).not.toExist();
    expect((await invokeWindow<TextFile>('main', 'file_read', file)).text).toBe('original\n');
    await browser.switchToWindow(source);
    await expect($('.editor-tab')).not.toExist();
    await expect($('.editor-pane')).not.toExist();
    await closeContentTestWindow(source);
    await browser.switchToWindow(destination);
    await $('button[aria-label="Save file (⌘S)"]').click();
    await expect($('.editor-status')).toHaveText(expect.stringContaining('Saved'));
    const saved = await invokeWindow<TextFile>('main', 'file_read', file);
    expect(saved.text).toContain('unsaved transfer');
    await invokeWindow('main', 'file_write', { ...file, revision: saved.revision, text: 'external edit\n' });
    await $('.monaco-editor textarea').addValue('local edit\n');
    await $('button[aria-label="Save file (⌘S)"]').click();
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('This file changed remotely'));
    await $('button=Discard my edits').click();
    await expect($('.view-lines')).toHaveText(expect.stringMatching(/external\s+edit/));
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/content-editor.png', import.meta.url).pathname,
    );
    await $('.monaco-editor textarea').addValue('close check\n');
    // The embedded WebDriver's closeWindow destroys the window, bypassing CloseRequested.
    const requestClose = () =>
      browser.tauri.execute(
        ({ core }) => core.invoke('plugin:window|close'),
        withExecuteOptions({ windowLabel: destination }),
      );
    await requestClose();
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('Save changes?'));
    await $('button=Cancel').click();
    await expect($('.view-lines')).toHaveText(expect.stringMatching(/close\s+check/));
    await requestClose();
    await $('button=Discard').click();
    await browser.waitUntil(async () => !(await browser.getWindowHandles()).includes(destination));
  } finally {
    for (const label of [source, destination])
      if ((await browser.getWindowHandles()).includes(label)) await closeContentTestWindow(label);
    await browser.switchToWindow(main);
  }
}
export async function closeContentTestWindow(label: string) {
  await browser.tauri.execute(
    ({ core }) => {
      setTimeout(() => {
        void core.invoke('close_window', { cancel: false });
      }, 50);
    },
    withExecuteOptions({ windowLabel: label }),
  );
  await browser.waitUntil(async () => !(await browser.getWindowHandles()).includes(label));
}
