import { $, browser, expect } from '@wdio/globals';
import { withExecuteOptions } from '@wdio/tauri-service';
import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { invokeWindow } from './invoke';
import { edgeScrollbar, holdScrollbar, openSelect, pointerAt } from './scroll-controls';
import { toolRecords } from './tool-records';
import { activityTimes } from './activity';
import { treeMenuHighlight } from './tree-menu';
import { mathRendering } from './math';
import { fileLinks } from './file-links';

describe('Shared controls in native WebKit', () => {
  let main: string;
  let label: string;
  after(async () => {
    // Fixture resources belong to a disposable window, never the next spec's Client.
    if (label)
      await browser.tauri.execute(
        ({ core }) => {
          setTimeout(() => {
            void core.invoke('close_window', { cancel: false });
          }, 50);
          return true;
        },
        withExecuteOptions({ windowLabel: label }),
      );
    if (main) await browser.switchToWindow(main);
  });
  before(async () => {
    await $('button=Settings').waitForDisplayed();
    main = await browser.getWindowHandle();
    label = await invokeWindow<string>('main', 'new_window');
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 2);
    await browser.switchToWindow((await browser.getWindowHandles()).find((handle) => handle !== main)!);
    await $('button=Settings').waitForDisplayed();
    await browser.execute(() => window.dispatchEvent(new Event('remote-codex:controls-fixture')));
    await $('button=Fixture settings').waitForDisplayed();
    await browser.execute(() => {
      // Physical input on a shared desktop must not interrupt scripted scrollbar gestures.
      for (const type of ['pointermove', 'pointerout', 'pointercancel', 'blur'])
        window.addEventListener(
          type,
          (event) => {
            if (event.isTrusted && (type !== 'blur' || event.target === window)) event.stopImmediatePropagation();
          },
          true,
        );
    });
  });
  it('renders formulas in both themes and completes streamed math', mathRendering);
  it('opens conversation links in the owning remote editor and preserves tabs across navigation', fileLinks);
  it('handles nested, horizontal and terminal scrolling without revealing on scroll', async () => {
    await edgeScrollbar('#fixture-input', 'y');
    await expect($('#fixture-native')).not.toHaveAttribute('data-scroll-y');
    await edgeScrollbar('#fixture-code', 'x');
    await edgeScrollbar('#fixture-native', 'y');
    await holdScrollbar('#fixture-native');
    await pointerAt('#fixture-short', 'y');
    await expect($('#fixture-short')).not.toHaveAttribute('data-scroll-y');
    await edgeScrollbar('#fixture-terminal .xterm-scrollable-element', 'y', '#fixture-terminal .xterm');
    await edgeScrollbar('#fixture-multiple', 'y');
    const changed = await browser.execute(() => {
      const input = document.querySelector<HTMLTextAreaElement>('#fixture-input')!;
      input.scrollTop = 80;
      return input.scrollTop;
    });
    expect(changed).toBeGreaterThan(0);
  });
  it('provides an accessible themed select and preserves disclosure contents', async () => {
    const trigger = '[aria-label="Fixture choice"]';
    await openSelect(trigger);
    await edgeScrollbar('.select-viewport', 'y');
    // Embedded driver emits a WebDriver code point for End; exercise the DOM key explicitly.
    await browser.execute(() =>
      document.activeElement!.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'End', code: 'End', bubbles: true, cancelable: true }),
      ),
    );
    await browser.waitUntil(() =>
      browser.execute(() => document.activeElement?.textContent?.startsWith('Workspace 34') ?? false),
    );
    await browser.keys('ArrowUp');
    await browser.waitUntil(() =>
      browser.execute(() => document.activeElement?.textContent?.startsWith('Workspace 32') ?? false),
    );
    await browser.keys('Enter');
    await expect($('#selected-choice')).toHaveText('32');
    await openSelect(trigger);
    await browser.keys('Escape');
    await expect($('#selected-choice')).toHaveText('32');
    expect(
      await browser.execute((selector) => document.activeElement === document.querySelector(selector), trigger),
    ).toBe(true);
    await $('summary=Advanced options').click();
    await $('input[aria-label="Retained input"]').setValue('still here');
    await $('summary=Advanced options').click();
    await $('summary=Advanced options').click();
    await expect($('input[aria-label="Retained input"]')).toHaveValue('still here');
  });
  it('covers every Settings section and portaled dialogs in both themes', async () => {
    await browser.setWindowSize(1000, 600);
    for (const theme of ['Light', 'Dark']) {
      await $('button=Fixture settings').click();
      await $(`button=${theme}`).click();
      await edgeScrollbar('.settings-content', 'y');
      await $('button=Codex').click();
      await $('summary=Codex executable').click();
      await edgeScrollbar('.settings-content', 'y');
      await $('button=MCP').click();
      await openSelect('[aria-label="MCP workspace"]');
      await edgeScrollbar('.select-viewport', 'y');
      await pointerAt('.select-viewport', 'y');
      await expect($('.select-viewport')).toHaveAttribute('data-scroll-y', '');
      await browser.executeAsync((done) => setTimeout(done, 180));
      const appearance = await browser.execute(() => {
        const viewport = document.querySelector<HTMLElement>('.select-viewport')!;
        return {
          outline: getComputedStyle(document.activeElement!).outlineStyle,
          width: getComputedStyle(viewport).scrollbarWidth,
          thumb: getComputedStyle(viewport, '::-webkit-scrollbar-thumb').backgroundColor,
          thumbVertical: getComputedStyle(viewport, '::-webkit-scrollbar-thumb:vertical').backgroundColor,
          scrollbar: getComputedStyle(viewport, '::-webkit-scrollbar').display,
          gutter: viewport.offsetWidth - viewport.clientWidth,
        };
      });
      expect(appearance.outline).toBe('none');
      expect(appearance.thumb).not.toBe('rgba(0, 0, 0, 0)');
      writeFileSync(
        path.resolve(`../../.artifacts/desktop-e2e/select-${theme.toLowerCase()}-appearance.json`),
        JSON.stringify(appearance, null, 2),
      );
      await browser.saveScreenshot(
        path.resolve(`../../.artifacts/desktop-e2e/settings-select-${theme.toLowerCase()}.png`),
      );
      await browser.keys('Escape');
      await $('button=Sessions').click();
      await expect($('[aria-label="Archived sessions"]')).toBeDisplayed();
      await $('[role="dialog"] button[aria-label="Close"]').click();
      await expect($('[data-scroll-y]')).not.toExist();
    }
    await $('button=Fixture modal').click();
    await edgeScrollbar('.modal', 'y');
    await $('[role="dialog"] button[aria-label="Close"]').click();
  });
  it('uses the same edge policy for Monaco and both diff panes', async () => {
    await $('button=Fixture editor').click();
    await $('.monaco-editor .view-lines').waitForDisplayed();
    await edgeScrollbar('.monaco-editor .monaco-scrollable-element', 'y');
    await edgeScrollbar('.monaco-editor .monaco-scrollable-element', 'x');
    await $('button=Fixture conflict').click();
    await $('.conflict-editor .modified .view-lines').waitForDisplayed();
    // Monaco switches to inline diff below its native breakpoint.
    await edgeScrollbar('.conflict-editor .modified .monaco-scrollable-element', 'y');
    await expect($('.diffOverview')).not.toExist();
    await browser.setWindowSize(1440, 900);
    await browser.execute(() => (document.querySelector<HTMLElement>('.modal')!.style.width = '1150px'));
    await browser.waitUntil(() =>
      browser.execute(
        () =>
          document.querySelector('.conflict-editor .original .monaco-scrollable-element')!.getBoundingClientRect()
            .width > 300,
      ),
    );
    await edgeScrollbar('.conflict-editor .original .monaco-scrollable-element', 'y');
    await edgeScrollbar('.conflict-editor .modified .monaco-scrollable-element', 'y');
    await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/scrolling-diff.png'));
    await $('[role="dialog"] button[aria-label="Close"]').click();
  });
  it('expands tool records with actual details and keeps them open during updates', async () => {
    await toolRecords((theme) => invokeWindow(label, 'app_preferences', { patch: { theme } }));
  });
  it('shows compact activity times without clipping them in narrow session rows', async () => {
    await activityTimes((theme) => invokeWindow(label, 'app_preferences', { patch: { theme } }));
  });
  it('keeps the menu target highlighted without changing the selected session', async () => {
    await treeMenuHighlight((theme) => invokeWindow(label, 'app_preferences', { patch: { theme } }));
  });
  it('allows a new message after exhausted recovery, retaining it on failure or cancellation', async () => {
    await $('button=Fixture recovery').click();
    const input = $('textarea[aria-label="Message Codex"]');
    const send = $('button[aria-label="Send message"]');
    const text = await input.getValue();
    await expect(send).toBeEnabled();
    await send.click();
    await expect(send).toBeDisabled();
    await expect($('button[aria-label="Cancel message"]')).toBeEnabled();
    await $('button=Fail reconnect').click();
    await expect(input).toHaveValue(text);
    await expect(send).toBeEnabled();
    await send.click();
    await input.click();
    await browser.keys('Escape');
    await expect(send).toBeEnabled();
    await expect(input).toHaveValue(text);
    await expect($('#recovery-submitted')).toHaveText('0');
    await send.click();
    await $('button=Restore connection').click();
    await expect($('#recovery-submitted')).toHaveText('1');
    await expect(input).toHaveValue('');
    // A setting change can also lose its connection; it is not a queued message.
    await input.setValue('/plan');
    await send.click();
    await expect(send).toBeDisabled();
    await expect($('button[aria-label="Cancel message"]')).not.toExist();
    await input.click();
    await browser.keys('Escape');
    await expect(send).toBeDisabled();
    await $('button=Restore connection').click();
    await expect($('button[aria-label="Permissions"]')).toBeEnabled();
    await expect($('#recovery-submitted')).toHaveText('1');
    await input.setValue(text);
    await $('button=Block identity').click();
    await expect(send).toBeDisabled();
    await $('button=Fixture recovery').click();
  });
});
