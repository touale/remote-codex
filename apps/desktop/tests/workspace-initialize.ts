import { $, browser, expect } from '@wdio/globals';
import path from 'node:path';
import { coldDirectoryBrowsing } from './directory-lifecycle';
import { activateTree, contextAt, rowMenu } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export async function initializeWorkspace(invoke: Invoke, remote: string) {
  await $('button[aria-label="Add server"]').waitForDisplayed();
  await $('button[aria-label="Add server"]').click();
  await $('input[placeholder="development"]').setValue('desktop-test');
  await $('input[placeholder="user@hostname"]').setValue(process.env.REMOTE_CODEX_E2E_TARGET!);
  await $('input[type="number"]').setValue(process.env.REMOTE_CODEX_E2E_PORT!);
  if (process.env.REMOTE_CODEX_E2E_IDENTITY)
    await $('input[placeholder="/Users/you/.ssh/id_ed25519"]').setValue(process.env.REMOTE_CODEX_E2E_IDENTITY);
  await $('[role="dialog"] button.primary').click();
  await $('[role="dialog"]').waitForExist({ reverse: true, timeout: 120000 });
  await coldDirectoryBrowsing(invoke, remote);
  const opened = await invoke<{ id: string }>('workspace_open', {
    operationId: crypto.randomUUID(),
    server: 'desktop-test',
    path: remote,
  });

  await $(`button[title="${remote}"]`).waitForDisplayed();
  await activateTree(`button[title="${remote}"]`);
  await expect($('.workspace-path')).toHaveText(remote);
  await expect($('.server-row')).toHaveText(expect.stringContaining(`:${process.env.REMOTE_CODEX_E2E_PORT})`));
  await expect($('.workspace-row .path-name')).toHaveText(remote.split('/').at(-1)!);
  await expect($('.server-row .lucide-folder-open')).toExist();
  await $('.server-row .tree-disclosure').click();
  await expect($('.server-row .lucide-folder')).toExist();
  await $('.server-row .tree-disclosure').click();
  await expect($('.server-row .lucide-folder-open')).toExist();
  expect(
    await browser.execute(
      () =>
        document.querySelector('.connections')!.getBoundingClientRect().width <=
        document.querySelector('aside')!.clientWidth,
    ),
  ).toBe(true);
  await $('.file-row .tree-label[title="cold-a"]').waitForDisplayed();
  for (const [selector, label] of [
    ['.server-row', 'Server settings…'],
    ['.workspace-row', 'Open in New Window'],
  ]) {
    await rowMenu(selector, label);
  }
  await contextAt('.workspace-row .tree-label');
  await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/workspace-context-menu.png'));
  await $('[role="menuitem"]=Remove workspace…').click();
  await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('Remove workspace?'));
  await $('button=Cancel').click();
  await expect($('.workspace-path')).toHaveText(remote);
  return opened.id;
}
