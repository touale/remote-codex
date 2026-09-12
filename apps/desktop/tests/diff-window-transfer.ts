import { $, browser, expect } from '@wdio/globals';
import { closeContentTestWindow } from './content-windows';

declare global {
  interface Window {
    diffWindowGate: { release?: (fail: boolean) => void; restore: () => void };
  }
}

export async function moveDiffView() {
  const source = await browser.getWindowHandle();
  const existing = await browser.getWindowHandles();
  await browser.execute(() => {
    const original = window.fetch;
    const gate: Window['diffWindowGate'] = { restore: () => (window.fetch = original) };
    window.diffWindowGate = gate;
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol !== 'ipc:' || url.pathname !== '/new_window') return original.call(window, input, init);
      const fail = await new Promise<boolean>((resolve) => {
        gate.release = resolve;
      });
      delete gate.release;
      return fail
        ? new Response(JSON.stringify({ code: 'WINDOW_ERROR', message: 'Fixture window creation failed.' }), {
            headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' },
          })
        : original.call(window, input, init);
    };
  });
  try {
    for (const scenario of ['failure', 'switched', 'success']) {
      await $('button[aria-label="Open changes in New Window"]').click();
      await browser.waitUntil(() => browser.execute(() => !!window.diffWindowGate.release));
      await expect($('.editor-pane .change-view')).toBeDisplayed();
      if (scenario === 'switched') {
        await $('button.diff-link=src/other.rs').click();
        await expect($('.editor-pane .diff-heading')).toHaveText(expect.stringContaining('other.rs'));
      }
      await browser.execute((fail) => window.diffWindowGate.release!(fail), scenario === 'failure');
      if (scenario === 'failure') {
        await expect($('[role="alert"]')).toHaveText('Fixture window creation failed.');
        await expect($('.editor-pane .diff-heading')).toHaveText(expect.stringContaining('main.rs'));
        expect(await browser.getWindowHandles()).toHaveLength(existing.length);
        continue;
      }
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length > existing.length);
      const target = (await browser.getWindowHandles()).find((label) => !existing.includes(label))!;
      try {
        await browser.switchToWindow(target);
        await expect($('.content-window .content-title strong')).toHaveText(
          `Changes · ${scenario === 'switched' ? 'main.rs' : 'other.rs'}`,
        );
        await expect($('.content-window .change-view')).toBeDisplayed();
        await browser.switchToWindow(source);
        if (scenario === 'switched')
          await expect($('.editor-pane .diff-heading')).toHaveText(expect.stringContaining('other.rs'));
        else await expect($('.editor-pane')).not.toExist();
        await expect($('button.diff-link=src/main.rs')).toExist();
        await expect($('button.diff-link=src/other.rs')).toExist();
      } finally {
        await closeContentTestWindow(target);
        await browser.switchToWindow(source);
      }
    }
  } finally {
    await browser.execute(() => {
      window.diffWindowGate.release?.(true);
      window.diffWindowGate.restore();
    });
  }
}
