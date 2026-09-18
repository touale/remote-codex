import { $, browser, expect } from '@wdio/globals';
import { contextAt, rowMenu } from './interaction-controls';

declare global {
  interface Window {
    copyPathFixture: { text: string | null; fail: boolean; restore: () => void };
  }
}

export async function copyFilePaths() {
  await browser.setWindowSize(1440, 900);
  await browser.execute(() => {
    const fetch = window.fetch;
    const descriptor = Object.getOwnPropertyDescriptor(navigator.clipboard, 'writeText');
    const state = (window.copyPathFixture = {
      text: null,
      fail: false,
      restore: () => {
        window.fetch = fetch;
        if (descriptor) Object.defineProperty(navigator.clipboard, 'writeText', descriptor);
        else Reflect.deleteProperty(navigator.clipboard, 'writeText');
      },
    } as Window['copyPathFixture']);
    Object.defineProperty(navigator.clipboard, 'writeText', {
      configurable: true,
      value: async (text: string) => {
        if (state.fail) throw new Error('Fixture clipboard unavailable.');
        state.text = text;
      },
    });
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol !== 'ipc:' || url.pathname !== '/file_list') return fetch.call(window, input, init);
      const { path } = JSON.parse(init!.body as string);
      const entries =
        path === 'src'
          ? [{ name: '中文 file.rs', path: 'src/中文 file.rs', directory: false, symlink: false, size: 1 }]
          : [
              { name: 'src', path: 'src', directory: true, symlink: false, size: 0 },
              { name: 'shortcut', path: 'shortcut', directory: false, symlink: true, size: 0 },
            ];
      return new Response(JSON.stringify({ entries, truncated: false }), {
        headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'ok' },
      });
    };
  });
  try {
    await $('button=Fixture files').click();
    const row = (path: string) => `.file-row[data-path="${path}"]`;
    await $(row('src')).waitForDisplayed();
    await rowMenu(row('src'), 'Copy path');
    const copy = async (path: string, label: string, expected: string) => {
      await browser.execute(() => {
        window.copyPathFixture.text = null;
      });
      await contextAt(row(path));
      await $(`[role="menuitem"]=${label}`).click();
      await browser.waitUntil(() => browser.execute((expected) => window.copyPathFixture.text === expected, expected));
      await expect($('[role="menu"]')).not.toExist();
    };
    await copy('src', 'Copy path', '/workspace/project/src');
    await copy('src', 'Copy relative path', 'src');
    await $(`${row('src')} .tree-label`).click();
    await $(row('src/中文 file.rs')).waitForDisplayed();
    await copy('src/中文 file.rs', 'Copy path', '/workspace/project/src/中文 file.rs');
    await copy('src/中文 file.rs', 'Copy relative path', 'src/中文 file.rs');
    await copy('shortcut', 'Copy path', '/workspace/project/shortcut');
    await $('button=Browse root').click();
    await copy('src/中文 file.rs', 'Copy path', '/src/中文 file.rs');
    await copy('src/中文 file.rs', 'Copy relative path', 'src/中文 file.rs');
    await browser.execute(() => {
      window.copyPathFixture.fail = true;
    });
    await contextAt(row('src'));
    await $('[role="menuitem"]=Copy path').click();
    await expect($('#file-tree-error')).toHaveText('Fixture clipboard unavailable.');
    await $('button=Clear root').click();
    await contextAt(row('src'));
    for (const label of ['Copy path', 'Copy relative path'])
      await expect($(`[role="menuitem"]=${label}`)).toHaveAttribute('data-disabled');
    await browser.keys('Escape');
  } finally {
    await $('button=Fixture files').click();
    await browser.execute(() => window.copyPathFixture.restore());
  }
}
