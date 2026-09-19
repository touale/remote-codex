import { $, browser, expect } from '@wdio/globals';
import { withExecuteOptions } from '@wdio/tauri-service';
import { closeContentTestWindow } from './content-windows';
import { invokeWindow } from './invoke';

const setFullscreen = (label: string, value: boolean) =>
  invokeWindow(label, 'plugin:window|set_fullscreen', { label, value });

async function titlebar(label: string) {
  return browser.tauri.execute(
    () => {
      const header = document.querySelector('main > header')!;
      const spacer = header.firstElementChild!;
      const first = spacer.nextElementSibling!;
      const title = [...header.children].find((node) => node.textContent === 'Remote Codex');
      return {
        fullscreen: document.documentElement.dataset.fullscreen,
        left: first.getBoundingClientRect().left - header.getBoundingClientRect().left,
        reserved: spacer.getBoundingClientRect().width,
        titleLeft: title?.getBoundingClientRect().left,
      };
    },
    withExecuteOptions({ windowLabel: label }),
  );
}

async function layout(label: string, fullscreen: boolean) {
  await browser.waitUntil(
    async () => {
      const value = await titlebar(label);
      return (
        value.fullscreen === String(fullscreen) &&
        value.reserved === (fullscreen ? 0 : 76) &&
        Math.abs(value.left - (fullscreen ? 10 : 82)) < 1
      );
    },
    { timeoutMsg: `Titlebar did not follow fullscreen=${fullscreen} in ${label}` },
  );
}

export async function fullscreenTitlebar() {
  await $('button=Settings').waitForDisplayed();
  const main = await browser.getWindowHandle();
  let content: string | undefined;
  try {
    await layout(main, false);
    const normal = await titlebar(main);
    await browser.setWindowSize(1100, 750);
    await layout(main, false);
    for (const fullscreen of [true, false, true, false]) {
      await setFullscreen(main, fullscreen);
      await layout(main, fullscreen);
      const current = await titlebar(main);
      expect(Math.abs(current.titleLeft! - normal.titleLeft! + (fullscreen ? 72 : 0))).toBeLessThan(1);
    }
    content = await invokeWindow<string>(main, 'new_window', {
      target: {
        kind: 'diff',
        document: {
          server: 'fixture',
          root: '/workspace',
          path: 'example.txt',
          diff: '@@ -1 +1 @@\n-before\n+after',
        },
      },
    });
    await browser.waitUntil(async () => (await browser.getWindowHandles()).includes(content!));
    await browser.switchToWindow(content);
    await $('.content-title').waitForDisplayed();
    await layout(content, false);
    await setFullscreen(content, true);
    await layout(content, true);
    await layout(main, false);
    await setFullscreen(content, false);
    await layout(content, false);
  } finally {
    if (content) {
      await setFullscreen(content, false);
      await closeContentTestWindow(content);
    }
    await browser.switchToWindow(main);
    await setFullscreen(main, false);
    await layout(main, false);
    await browser.setWindowSize(1440, 900);
  }
}
