import { $, browser, expect } from '@wdio/globals';
import type { TextFile } from '../src/bridge/types';
import { contextAt, rowMenu } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function fileCreation(invoke: Invoke, workspace: string, remote: string) {
  await contextAt('.connections .tree-scroll', false);
  await expect($('[role="menuitem"]=Add server…')).toBeDisplayed();
  await browser.keys('Escape');
  await expect($('.tree-add-server')).toBeDisplayed();
  await contextAt('.file-tree .tree-scroll', false);
  await $('[role="menuitem"]=New folder…').click();
  await $('[role="dialog"] input').setValue('assets');
  await $('button=Create').click();
  await $('.file-row [title="assets"]').waitForDisplayed();
  await rowMenu('.file-row:has([title="assets"])', 'New folder…');
  await contextAt('.file-row [title="assets"]', false);
  await $('[role="menuitem"]=New folder…').click();
  await expect($('[role="dialog"]')).toHaveText(expect.stringContaining(`${remote}/assets`));
  await $('[role="dialog"] input').setValue('nested');
  await $('button=Create').click();
  await $('.file-row [title="assets/nested"]').waitForDisplayed();
  await contextAt('.file-row [title="assets/nested"]');
  await $('[role="menuitem"]=New file…').click();
  await $('[role="dialog"] input').setValue('child.txt');
  await $('button=Create').click();
  await $('button[aria-label="Close assets/nested/child.txt"]').waitForDisplayed();
  await expect($('.file-row [title="assets/nested/child.txt"]')).toBeDisplayed();
  expect((await invoke<TextFile>('file_read', { context: workspace, path: 'assets/nested/child.txt' })).text).toBe('');
  await $('button[aria-label="Close assets/nested/child.txt"]').click();
  await expect($('.editor-pane')).not.toExist();
  await expect($('button[aria-label="Open a file to show the editor"]')).toBeDisabled();
  await expect($('.terminal-panel')).not.toBeDisplayed();
}
