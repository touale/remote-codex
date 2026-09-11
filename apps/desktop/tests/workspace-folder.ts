import { $, browser, expect } from '@wdio/globals';
import path from 'node:path';
import type { Catalog, DirectoryPage } from '../src/bridge/types';
import { activateTree, contextAt } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function workspaceFolderCreation(invoke: Invoke, remote: string) {
  const before = await invoke<Catalog>('catalog', { archived: false });
  await contextAt('.server-row .tree-label');
  await $('[role="menuitem"]=Add workspace…').click();
  const location = $('.workspace-dialog input[placeholder="/workspace/project"]');
  await location.setValue(remote);
  await $('button=Browse').click();
  await $('button=New folder').waitForEnabled();
  // Editing the address cannot accidentally create in the previously listed parent.
  await location.setValue(`${remote}/missing`);
  await expect($('[aria-label="Remote folders"]')).not.toExist();
  await expect($('button=New folder')).not.toExist();
  await $('button=Browse').click();
  await expect($('.workspace-dialog [role="alert"]')).toBeDisplayed();
  await location.setValue(remote);
  await $('button=Browse').click();
  await $('button=New folder').click();
  const name = $('.new-workspace-folder input');
  await name.setValue('../escape');
  await $('button=Create folder').click();
  await expect($('.new-workspace-folder [role="alert"]')).toHaveText(expect.stringContaining('single folder name'));
  await expect(name).toHaveValue('../escape');
  const folder = 'New project 中文';
  await name.setValue(folder);
  await expect(location).toBeDisabled();
  await expect($('button=Open workspace')).toBeDisabled();
  await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/add-workspace-new-folder.png'));
  await $('button=Create folder').click();
  await expect(location).toHaveValue(`${remote}/${folder}`);
  await expect($('.new-workspace-folder')).not.toExist();
  await $('button=New folder').waitForEnabled();
  const handle = await invoke<string>('directory_open', { operationId: crypto.randomUUID(), server: 'desktop-test' });
  const listing = await invoke<DirectoryPage>('directory_browse', {
    operationId: crypto.randomUUID(),
    browser: handle,
    path: remote,
  });
  await invoke('directory_close', { browser: handle });
  expect(listing.entries.some((entry) => entry.name === folder && entry.directory)).toBe(true);
  expect((await invoke<Catalog>('catalog', { archived: false })).workspaces).toEqual(before.workspaces);
  await $('button=Parent directory').click();
  await $('button=New folder').click();
  await name.setValue(folder);
  await $('button=Create folder').click();
  await expect($('.new-workspace-folder [role="alert"]')).toHaveText(expect.stringContaining('Cannot create'));
  await expect(name).toHaveValue(folder);
  await expect(location).toHaveValue(remote);
  await $('button=Cancel new folder').click();
  await $(`.workspace-dialog .picker-row span[title="${folder}"]`).click();
  await $('button=Open workspace').waitForEnabled();
  await $('button=Open workspace').click();
  await $('.workspace-dialog').waitForExist({ reverse: true });
  await expect($('.workspace-path')).toHaveText(`${remote}/${folder}`);
  await contextAt(`[data-workspace-path="${remote}/${folder}"] .tree-label`);
  await $('[role="menuitem"]=Remove workspace…').click();
  await $('button=Remove workspace').click();
  await $('[role="dialog"]').waitForExist({ reverse: true });
  await activateTree(`[data-workspace-path="${remote}"] .tree-label`);
  await expect($('.workspace-path')).toHaveText(remote);
}
