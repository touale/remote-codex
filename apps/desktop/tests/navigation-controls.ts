import { $, browser, expect } from '@wdio/globals';
import { contextAt } from './interaction-controls';

export async function newSession() {
  const path = await $('.workspace-path').getAttribute('title');
  await contextAt(`[data-workspace-path=${JSON.stringify(path)}] .tree-label`);
  await $('[role="menuitem"]=New session').click();
  await $('textarea[aria-label="Message Codex"]').waitForDisplayed();
}

export async function refreshTree() {
  const before = await $('textarea[aria-label="Message Codex"]').getValue();
  const selected = await $('footer').getAttribute('data-session-id');
  const path = await $('.workspace-path').getText();
  await $('button[aria-label="Refresh workspaces"]').click();
  await $('button[aria-label="Refresh workspaces"]').waitForEnabled();
  await expect($('textarea[aria-label="Message Codex"]')).toHaveValue(before);
  await expect($('footer')).toHaveAttribute('data-session-id', selected!);
  await expect($('.workspace-path')).toHaveText(path);
  await contextAt('.connections .tree-scroll');
  await expect($('[role="menuitem"]=Refresh')).toBeDisplayed();
  await expect($('[role="menuitem"]=Show archived sessions')).not.toExist();
  await browser.keys('Escape');
  await expect($('button[aria-label="New session (⌘N)"]')).not.toExist();
  expect(
    await browser.execute(() => document.querySelector('.composer-options button')!.getAttribute('aria-label')),
  ).toBe('Session status (/status)');
  expect(
    await browser.execute(() => {
      const server = document.querySelector('.server-row .tree-disclosure svg')!.getBoundingClientRect();
      const add = document.querySelector('.tree-add-server svg')!.getBoundingClientRect();
      return Math.abs(server.left - add.left);
    }),
  ).toBeLessThanOrEqual(1);
}
