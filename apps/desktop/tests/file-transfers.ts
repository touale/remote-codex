import { $, browser, expect } from '@wdio/globals';
import type { Catalog, Failure } from '../src/bridge/types';
import { contextAt } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export async function fileTransfers(invoke: Invoke, workspace: string, remote: string) {
  await expect($('button[aria-label="Workspace actions"]')).not.toExist();
  await expect($('.workspace-row .node-kind')).not.toExist();
  await contextAt('.file-tree .tree-scroll', false);
  await expect($('[role="menuitem"]=Upload files…')).toBeDisplayed();
  await expect($('[role="menuitem"]=Upload folder…')).toBeDisplayed();
  await browser.keys('Escape');
  await contextAt('.file-row .tree-label[title="notes.md"]');
  await expect($('[role="menuitem"]=Download…')).toBeDisplayed();
  await expect($('[role="menuitem"]=Upload files…')).toBeDisplayed();
  await browser.keys('Escape');
  await invoke('file_change', { context: workspace, change: { action: 'directory', path: 'transfer-menu' } });
  await $('button[aria-label="Refresh files"]').click();
  await $('.file-row .tree-label[title="transfer-menu"]').waitForDisplayed();
  await contextAt('.file-row .tree-label[title="transfer-menu"]');
  await expect($('[role="menuitem"]=Download…')).toBeDisplayed();
  await expect($('[role="menuitem"]=Upload folder…')).toBeDisplayed();
  await browser.keys('Escape');
  const invalid = await invoke('transfer_upload', {
    context: workspace,
    destination: '',
    token: crypto.randomUUID(),
    paths: ['/etc/passwd'],
  }).then(
    () => null,
    (error: Failure) => error,
  );
  expect(invalid?.code).toBe('RESOURCE_UNAVAILABLE');
  expect(await invoke('transfer_list')).toEqual([]);
  const before = await $('.workspace-path').getText();
  const { servers } = await invoke<Catalog>('catalog', { archived: false });
  const row = `.server-row[data-server-id="${servers[0].id}"] .tree-label`;
  await $(row).click();
  await expect($('.workspace-path')).toHaveText(before);
  await $(`[data-workspace-path="${remote}/projects/test1"] .tree-label`).click();
  await expect($('.workspace-path')).toHaveText(before);
  const style = await browser.execute(() => {
    const name = document.querySelector('.server-row strong')!;
    const address = document.querySelector('.server-row .node-detail')!;
    return { nameShrink: getComputedStyle(name).flexShrink, addressShrink: getComputedStyle(address).flexShrink };
  });
  expect(style).toEqual({ nameShrink: '0', addressShrink: '1' });
}
