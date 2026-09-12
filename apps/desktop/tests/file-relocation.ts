import { $, browser, expect } from '@wdio/globals';
import type { TextFile } from '../src/bridge/types';
import { contextAt } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
const row = (path: string) => `.file-row .tree-label[title="${path}"]`;

export async function fileRelocation(invoke: Invoke, context: string) {
  const selected = await $('.editor-tab.selected button[role="tab"]').getAttribute('title');
  for (const path of ['relocate', 'relocate/empty', 'relocate/destination'])
    await invoke('file_change', { context, change: { action: 'directory', path } });
  await invoke('file_write', { context, path: 'relocate/source.txt', text: 'original', revision: null });
  await $('button[aria-label="Refresh files"]').click();
  await $(row('relocate')).click();
  await $(row('relocate/empty')).click();
  await expect($(row('relocate/empty'))).toHaveAttribute('aria-expanded', 'true');
  // This shell mutation emits no App notification. Full refresh must invalidate child caches.
  await $('button[aria-label="Toggle terminal (⌘`)"]').click();
  await $('.xterm-helper-textarea').waitForExist();
  await $('.xterm-helper-textarea').addValue('mv relocate/source.txt relocate/empty/source.txt\r');
  await browser.waitUntil(async () => {
    try {
      return (await invoke<TextFile>('file_read', { context, path: 'relocate/empty/source.txt' })).text === 'original';
    } catch {
      return false;
    }
  });
  await $('button[aria-label="Minimize terminal"]').click();
  await $('button[aria-label="Refresh files"]').click();
  await $(row('relocate/empty/source.txt')).waitForDisplayed();
  await expect($(row('relocate/source.txt'))).not.toExist();
  await $(row('relocate/empty/source.txt')).click();
  await $('.monaco-editor textarea').waitForExist();
  await $('.monaco-editor textarea').addValue('unsaved ');
  await contextAt(row('relocate/empty/source.txt'));
  await $('[role="menuitem"]=Rename…').click();
  await expect($('[role="dialog"] input')).toHaveValue('source.txt');
  await $('[role="dialog"] input').setValue('renamed.txt');
  await $('button=Rename').click();
  await expect($('.editor-tab.selected .tab-name')).toHaveText('renamed.txt');
  await expect($('.editor-tab.selected .dirty-dot')).toExist();
  await $(row('relocate/empty/renamed.txt')).waitForDisplayed();
  await contextAt(row('relocate/empty'));
  await $('[role="menuitem"]=Move to…').click();
  await $('[aria-label="Destination folders"]').$('button=destination').click();
  await $('button=Move here').waitForEnabled();
  await $('button=Move here').click();
  await $('[role="dialog"]').waitForExist({ reverse: true });
  await $(row('relocate/destination/empty/renamed.txt')).waitForDisplayed();
  await expect($('.editor-tab.selected .dirty-dot')).toExist();
  await $('button[aria-label="Save file (⌘S)"]').click();
  await browser.waitUntil(async () =>
    (await invoke<TextFile>('file_read', { context, path: 'relocate/destination/empty/renamed.txt' })).text.includes(
      'unsaved',
    ),
  );
  // Internal HTML drag shares the same move operation; external OS drops have a separate upload route.
  await browser.execute(() => {
    const source = document.querySelector<HTMLElement>('[data-path="relocate/destination/empty/renamed.txt"]')!;
    const destination = document.querySelector<HTMLElement>('[data-path="relocate"]')!;
    const dataTransfer = new DataTransfer();
    for (const [element, type] of [
      [source, 'dragstart'],
      [destination, 'dragover'],
      [destination, 'drop'],
      [source, 'dragend'],
    ] as const)
      element.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer }));
  });
  await $(row('relocate/renamed.txt')).waitForDisplayed();
  await expect($('.editor-tab.selected button[role="tab"]')).toHaveAttribute(
    'title',
    expect.stringContaining('/relocate/renamed.txt'),
  );
  await $('.monaco-editor textarea').addValue('Incoming draft. ');
  await invoke('file_write', { context, path: 'relocate/target.txt', text: 'target', revision: null });
  await $('button[aria-label="Refresh files"]').click();
  await $(row('relocate/target.txt')).click();
  await expect($('.editor-tab.selected .tab-name')).toHaveText('target.txt');
  await expect($('.monaco-editor .view-lines')).toHaveText('target');
  await $('.monaco-editor textarea').addValue('Destination draft. ');
  await contextAt(row('relocate/renamed.txt'));
  await $('[role="menuitem"]=Rename…').click();
  await $('[role="dialog"] input').setValue('target.txt');
  await $('button=Rename').click();
  await expect($('[role="alert"]:has(button[aria-label="Dismiss error"])')).toHaveText(
    expect.stringContaining('Resolve unsaved changes in the destination tab'),
  );
  await $('button[aria-label="Dismiss error"]').click();
  // A native relocation bypasses this window's preflight, as a move in another window would.
  await invoke('file_change', { context, change: { action: 'remove', path: 'relocate/target.txt' } });
  await invoke('file_change', {
    context,
    change: { action: 'rename', path: 'relocate/renamed.txt', destination: 'relocate/target.txt' },
  });
  await $('.editor-tabs').$('button[role="tab"][title$="/relocate/renamed.txt"]').click();
  await $('button=Retry move').waitForDisplayed();
  await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/Incoming\s+draft\./));
  await expect($('button[aria-label="Save file (⌘S)"]')).toBeDisabled();
  await $('.editor-tabs').$('button[role="tab"][title$="/relocate/target.txt"]').click();
  await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/Destination\s+draft\./));
  await $('button[aria-label="Close relocate/target.txt"]').click();
  await $('button=Discard').click();
  await $('button=Retry move').click();
  await $('button=Retry move').waitForExist({ reverse: true });
  await expect($('.editor-tab.selected .tab-name')).toHaveText('target.txt');
  await $('button[aria-label="Save file (⌘S)"]').click();
  await browser.waitUntil(async () =>
    (await invoke<TextFile>('file_read', { context, path: 'relocate/target.txt' })).text.includes('Incoming draft.'),
  );
  await $('button[aria-label="Close relocate/target.txt"]').click();
  await $(`.editor-tab button[role="tab"][title=${JSON.stringify(selected)}]`).click();
}
