import { $, browser, expect } from '@wdio/globals';
import type { Catalog } from '../src/bridge/types';
import { contextAt } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function archiveAndRestore(invoke: Invoke, id: string) {
  await contextAt(`[data-session-id="${id}"] .tree-label`);
  await $('[role="menuitem"]=Archive').click();
  await $('button=Close and continue').click();
  await $(`.session-row[data-session-id="${id}"]`).waitForExist({ reverse: true });
  const before = await invoke<Catalog>('catalog', { archived: false });
  await $('button=Settings').click();
  await $('button=Sessions').click();
  const row = `[data-archived-session="${id}"]`;
  await $(row).waitForDisplayed();
  await expect($(row)).toHaveText(expect.stringContaining('desktop-test'));
  await $('input[aria-label="Search archived sessions"]').setValue('no-such-session');
  await expect($('[aria-label="Archived sessions"]')).toHaveText(expect.stringContaining('No matching sessions'));
  await $('input[aria-label="Search archived sessions"]').setValue('desktop-test');
  await $(row).waitForDisplayed();
  await browser.saveScreenshot(
    new URL('../../../.artifacts/desktop-e2e/archived-sessions.png', import.meta.url).pathname,
  );
  await $(row).$('button=Restore').click();
  await $(row).waitForExist({ reverse: true });
  await expect($('[aria-label="Archived sessions"]')).toHaveText(expect.stringContaining('No matching sessions'));
  const after = await invoke<Catalog>('catalog', { archived: false });
  expect(after.live.map((item) => item.session.id)).toEqual(before.live.map((item) => item.session.id));
  expect(after.sessions.some((item) => item.session.id === id)).toBe(true);
  await $('[role="dialog"] button[aria-label="Close"]').click();
  await $(`[data-session-id="${id}"] .tree-label`).waitForDisplayed();
  await expect($('button=Resume session')).toBeDisplayed();
}
