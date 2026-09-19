import { $, browser, expect } from '@wdio/globals';
import path from 'node:path';
import { contextAt } from './interaction-controls';
import { sharedAppearanceAndUsage } from './settings-controls';
import { fullscreenTitlebar } from './fullscreen';

describe('Remote Codex desktop', () => {
  it('removes traffic light spacing only while each native window is fullscreen', fullscreenTitlebar);
  it('offers Remote Codex update settings without connecting a server', async () => {
    await $('button=Settings').waitForDisplayed();
    await $('button=Settings').click();
    await $('button=Updates').click();
    await $('#update-mode').waitForDisplayed();
    await expect($('button=Check for Updates')).toBeEnabled();
    await expect($('[role=dialog]')).toHaveText(expect.stringContaining('App version'));
    await $('#update-mode').click();
    await $('[role="option"]=Check manually').click();
    await expect($('#update-mode')).toHaveText('Check manually');
    await browser.waitUntil(
      async () =>
        (await browser.tauri.execute(async ({ core }) => (await core.invoke('update_status')) as { mode: string }))
          .mode === 'manual',
    );
    await $('[role=dialog] button[aria-label=Close]').click();
    await $('button=Settings').click();
    await $('button=Updates').click();
    await expect($('#update-mode')).toHaveText('Check manually');
    await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/updates.png'));
    await browser.tauri.execute(async ({ core }) => {
      await core.invoke('update_configure', { mode: 'notify' });
    });
    await expect($('#update-mode')).toHaveText('Check automatically');
    await $('[role=dialog] button[aria-label=Close]').click();
  });
  it('opens a compact native window with both sidebar trees', async () => {
    await $('section[aria-label="Connections and sessions"]').waitForDisplayed();
    await expect($('section[aria-label="Remote files"]')).toBeDisplayed();
    await expect($('button[aria-label="Refresh files"]')).toBeDisabled();
    await expect($('button[aria-label="Refresh files"] svg')).not.toHaveElementClass('spinning');
    await expect($('body')).not.toHaveText(expect.stringContaining('RESOURCE_UNAVAILABLE'));
    await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/light.png'));
    await expect($('[aria-label="Start working"]')).toHaveText(expect.stringContaining('Welcome to Remote Codex'));
    await expect($('button[aria-label="Open a file to show the editor"]')).toBeDisabled();
    await expect($('.editor-pane')).not.toExist();
    await expect($('.terminal-panel')).not.toBeDisplayed();
    await contextAt('.connections .tree-scroll', false);
    await expect($('[role="menuitem"]=Add server…')).toBeDisplayed();
    await browser.keys('Escape');
    await contextAt('.file-tree .tree-scroll', false);
    await expect($('[role="menuitem"]=New folder…')).toHaveAttribute('data-disabled');
    await browser.keys('Escape');
  });
  it('offers graphical server setup with direct connections as the default', async () => {
    await $('button[aria-label="Add server"]').click();
    await $('[role="dialog"]').waitForDisplayed();
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('SSH address'));
    await expect($('input[type="checkbox"]')).not.toBeSelected();
    await $('button[aria-label="Close"]').click();
  });
  it('persists dark appearance and supports collapsing panels', async () => {
    await $('button=Settings').click();
    await $('[role="dialog"]').waitForDisplayed();
    await $('button=Dark').click();
    await expect($('html')).toHaveAttribute('data-theme', 'dark');
    await browser.waitUntil(
      async () =>
        (await browser.tauri.execute(
          async ({ core }) => ((await core.invoke('app_preferences', { patch: null })) as { theme: string }).theme,
        )) === 'dark',
    );
    await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/settings-general-dark.png'));
    await $('[role=dialog] button[aria-label=Close]').click();
    await $('button[aria-label="Toggle terminal (⌘`)"]').click();
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('Choose a server'));
    await $('button=Cancel').click();
    const settingsBottom = await browser.execute(
      () => document.querySelector('aside > div:last-child')!.getBoundingClientRect().bottom,
    );
    await $('button=Files').click();
    expect(
      await browser.execute(() => document.querySelector('aside > div:last-child')!.getBoundingClientRect().bottom),
    ).toBe(settingsBottom);
    await $('button=Workspaces').click();
    await expect($('section[aria-label="Connections and sessions"] .section-title')).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    await $('button=Files').click();
    expect(
      await browser.execute(
        () =>
          document.querySelector('section[aria-label="Remote files"]')!.getBoundingClientRect().top >=
          document.querySelector('aside')!.getBoundingClientRect().top,
      ),
    ).toBe(true);
    await $('button=Workspaces').click();
    await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/dark.png'));
    await $('button[aria-label="Toggle sidebar (⌘B)"]').click();
    await expect($('section[aria-label="Connections and sessions"]')).not.toBeExisting();
    await $('button[aria-label="Toggle sidebar (⌘B)"]').click();
  });
  it('shares General preferences across windows and shows local account usage', sharedAppearanceAndUsage);
  it('rejects unknown resource handles through the real application API', async () => {
    const result = await browser.tauri.execute(async ({ core }) => {
      try {
        await core.invoke('terminal_input', { id: 'unowned-resource', bytes: [65] });
        return 'unexpected success';
      } catch (error) {
        return (error as { code: string }).code;
      }
    });
    expect(result).toBe('RESOURCE_UNAVAILABLE');
  });
});
