import { $, browser, expect } from '@wdio/globals';
import { withExecuteOptions } from '@wdio/tauri-service';
import path from 'node:path';
import type { AppPreferences } from '../src/bridge/preferences';
import type { SessionSnapshot } from '../src/bridge/types';
import { invokeWindow } from './invoke';
export async function sharedAppearanceAndUsage() {
  let windowLabel = 'main';
  const invoke = <T>(command: string, args: Record<string, unknown> = {}) =>
    invokeWindow<T>(windowLabel, command, args);
  const main = await browser.getWindowHandle();
  await $('button=Settings').click();
  await $('button=Light').click();
  await $('#setting-weekly_limit').click();
  await $('#setting-session_tokens').click();
  await browser.waitUntil(
    async () => (await invoke<AppPreferences>('app_preferences', { patch: null })).session_tokens,
  );
  await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/settings-general-light.png'));
  await $('[role=dialog] button[aria-label=Close]').click();
  const label = await invoke<string>('new_window', { workspace: null });
  await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 2);
  const other = (await browser.getWindowHandles()).find((id) => id !== main)!;
  await browser.switchToWindow(other);
  windowLabel = label;
  await $('button=Settings').waitForDisplayed();
  await expect($('html')).toHaveAttribute('data-theme', 'light');
  await $('button=Settings').click();
  await expect($('#setting-weekly_limit')).toHaveAttribute('data-state', 'checked');
  await $('#setting-weekly_limit').click();
  await $('button=Dark').click();
  await browser.waitUntil(
    async () => (await invoke<AppPreferences>('app_preferences', { patch: null })).theme === 'dark',
  );
  await $('[role=dialog] button[aria-label=Close]').click();
  await browser.switchToWindow(main);
  windowLabel = 'main';
  await expect($('html')).toHaveAttribute('data-theme', 'dark');
  await $('button=Settings').click();
  await expect($('#setting-weekly_limit')).toHaveAttribute('data-state', 'unchecked');
  await expect($('#setting-session_tokens')).toHaveAttribute('data-state', 'checked');
  await $('#setting-session_tokens').click();
  await $('[role=dialog] button[aria-label=Close]').click();
  await browser.switchToWindow(other);
  await browser.tauri.execute(
    ({ core }) => {
      setTimeout(() => {
        void core.invoke('close_window', { cancel: false });
      }, 50);
      return true;
    },
    withExecuteOptions({ windowLabel: label }),
  );
  await browser.switchToWindow(main);
  await browser.refresh();
  await $('button=Settings').waitForDisplayed();
  await expect($('html')).toHaveAttribute('data-theme', 'dark');
  await $('button=Settings').click();
  await expect($('#setting-five_hour_limit')).toHaveAttribute('data-state', 'checked');
  await expect($('#setting-context_usage')).toHaveAttribute('data-state', 'checked');
  await expect($('#setting-weekly_limit')).toHaveAttribute('data-state', 'unchecked');
  await expect($('#setting-session_tokens')).toHaveAttribute('data-state', 'unchecked');
  await $('button=Codex').click();
  await $('.account-usage').waitForDisplayed();
  await browser.waitUntil(async () => !(await $('.account-usage').getText()).includes('Checking usage limits'));
  await expect($('.account-usage')).toHaveText(expect.stringContaining('Usage limits'));
  await expect($('.account-usage')).not.toHaveText(expect.stringContaining('100% left'));
  await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/settings-codex.png'));
  await $('[role=dialog] button[aria-label=Close]').click();
  await expect($('footer')).toHaveText(expect.stringContaining('No workspace selected'));
}

export async function startupWithoutConnection(
  remote: string,
  session: string,
  usage: SessionSnapshot['status']['usage'],
) {
  let windowLabel = 'main';
  const invoke = <T>(command: string, args: Record<string, unknown> = {}) =>
    invokeWindow<T>(windowLabel, command, args);
  // A new empty window has no connection intent, even when it later remembers a workspace.
  const main = await browser.getWindowHandle();
  const label = await invoke<string>('new_window');
  await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 2);
  await browser.switchToWindow((await browser.getWindowHandles()).find((id) => id !== main)!);
  windowLabel = label;
  await $('section[aria-label="Connections and sessions"]').waitForDisplayed();
  const prefs = await invoke<Record<string, unknown>>('preferences', { value: null });
  await invoke('preferences', {
    value: { ...prefs, selected_workspace: ['desktop-test', remote], editor_visible: true, terminal_visible: true },
  });
  await browser.refresh();
  await browser.waitUntil(async () => (await $('footer').getText()).includes('No workspace selected'));
  // The ready footer is the completion signal; background WebViews can suspend RAF.
  await expect($('.workspace-path')).toHaveText('No location selected');
  await expect($('[aria-label="Start working"]')).toBeDisplayed();
  await expect($('[aria-label="Recent sessions"]')).toBeDisplayed();
  await expect($('.editor-pane')).not.toExist();
  await expect($('.terminal-panel')).not.toBeDisplayed();
  await expect($('[role="dialog"]')).not.toExist();
  await expect($('footer')).toHaveText(expect.stringContaining('No workspace selected'));
  // A fresh window owns a fresh Client and native process: no previous frontend usage cache.
  await $(`.session-row[data-session-id="${session}"] .tree-label`).click();
  await $('textarea[aria-label="Message Codex"]').waitForDisplayed({ timeout: 60000 });
  const restored = await invoke<SessionSnapshot>('session_snapshot', { id: session });
  expect(restored.status.usage).toEqual(usage);
  expect(restored.status.usage?.total_tokens).toBeGreaterThan(0);
  await expect($('.usage-strip')).not.toHaveText(expect.stringContaining('Context —'));
  await $('button[aria-label="Session status (/status)"]').click();
  await expect($('.session-status')).toHaveText(expect.stringContaining('Session total'));
  await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/restored-usage.png'));
  await $('[role="dialog"] button[aria-label="Close"]').click();
  await browser.tauri.execute(({ core }) => {
    setTimeout(() => {
      void core.invoke('close_window', { cancel: false });
    }, 50);
    return true;
  }, withExecuteOptions({ windowLabel }));
  windowLabel = 'main';
  await browser.switchToWindow(main);
}
