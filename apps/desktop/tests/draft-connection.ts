import { $, browser, expect } from '@wdio/globals';
import type { Catalog } from '../src/bridge/types';
import { heldFile, holdFiles, releaseFile, restoreFiles } from './file-gate';
import { contextAt } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
declare global {
  interface Window {
    draftSessionOpens: number;
  }
}

// Runs in a fresh window before any workspace has connected in that window.
export async function draftConnection(invoke: Invoke, remote: string) {
  const input = 'textarea[aria-label="Message Codex"]';
  const files = $('.file-tree');
  const before = await invoke<Catalog>('catalog', { archived: false });
  const choose = async (path: string) => {
    await contextAt(`[data-workspace-path=${JSON.stringify(path)}] .tree-label`);
    await $('[role="menuitem"]=New session').click();
  };
  await holdFiles('desktop-test', 'workspace_open');
  await browser.execute(() => {
    const original = window.fetch;
    window.draftSessionOpens = 0;
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol === 'ipc:' && url.pathname === '/session_open') {
        window.draftSessionOpens++;
        return new Response(
          JSON.stringify({
            code: 'SETUP_INTERRUPTED',
            message: 'Fixture stopped before starting Codex.',
            outcome_unknown: false,
          }),
          { headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' } },
        );
      }
      return original.call(window, input, init);
    };
  });
  try {
    await $('button[aria-label="New session"]').click();
    await $('[role="dialog"]').$(`span=desktop-test · ${remote}`).click();
    const first = await heldFile('workspace_open', remote);
    await expect($(input)).toBeDisplayed();
    await expect($('[aria-label="Loading files"]')).toBeDisplayed();
    await expect(files).not.toHaveText(expect.stringContaining('Connect files'));
    await expect($('footer [role="status"]')).toHaveAttribute('data-state', 'busy');
    await $(input).setValue('Keep this draft while connecting.');
    await choose(remote);
    expect(await browser.execute(() => window.fileGate.held.length)).toBe(1);
    await releaseFile(first, true);
    await expect(files.$('[role="alert"]')).toHaveText('Fixture file operation interrupted.');
    await expect($(input)).toHaveValue('Keep this draft while connecting.');
    await expect($('footer [role="status"]')).toHaveAttribute('data-state', 'attention');
    await files.$('button=Retry').click();
    const retry = await heldFile('workspace_open', remote, first);
    await choose(`${remote}/second`);
    const other = await heldFile('workspace_open', `${remote}/second`);
    await releaseFile(retry, true);
    await expect(files.$('[role="alert"]')).not.toExist();
    await expect($('[aria-label="Loading files"]')).toBeDisplayed();
    await releaseFile(other);
    await $('[aria-label="Loading files"]').waitForExist({ reverse: true });
    await expect($('.workspace-path')).toHaveText(`${remote}/second`);
    await choose(remote);
    const final = await heldFile('workspace_open', remote, retry);
    await expect($(input)).toHaveValue('Keep this draft while connecting.');
    await $('button[aria-label="Send message"]').waitForEnabled();
    await $('button[aria-label="Send message"]').click();
    await expect($('[aria-label="Preparing first message"]')).toBeDisplayed();
    expect(await browser.execute(() => window.draftSessionOpens)).toBe(0);
    expect(await browser.execute(() => window.fileGate.held.length)).toBe(4);
    await releaseFile(final);
    await $('.submission-error').waitForDisplayed();
    await $('[aria-label="Loading files"]').waitForExist({ reverse: true });
    await expect($('.workspace-path')).toHaveText(remote);
    await expect(files.$('[role="alert"]')).not.toExist();
    await expect($('[data-pending-submission]')).toHaveText(
      expect.stringContaining('Keep this draft while connecting.'),
    );
    expect(await browser.execute(() => window.draftSessionOpens)).toBe(1);
    expect((await invoke<Catalog>('catalog', { archived: false })).live.length).toBe(before.live.length);
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/draft-auto-connect.png', import.meta.url).pathname,
    );
  } finally {
    await restoreFiles();
  }
}
