import { $, browser, expect } from '@wdio/globals';
import type { UpdateSnapshot } from '../src/bridge/updates';

declare global {
  interface Window {
    appVersionFixture: {
      snapshot: UpdateSnapshot;
      fail: boolean;
      hold: boolean;
      release?: () => void;
      restore: () => void;
    };
  }
}

export async function appVersionIndicator() {
  await browser.execute(() => {
    const original = window.fetch;
    const theme = document.documentElement.dataset.theme;
    const width = document.querySelector<HTMLElement>('aside')!.style.width;
    window.appVersionFixture = {
      snapshot: {
        current_version: '1.1.1',
        installed_version: '1.1.1',
        mode: 'manual',
        last_checked: null,
        latest: null,
        path: '/fixture/Remote Codex.app',
        can_install: true,
        busy: false,
        available: false,
        restart_required: false,
      },
      fail: false,
      hold: false,
      restore: () => {
        window.fetch = original;
        if (theme) document.documentElement.dataset.theme = theme;
        document.querySelector<HTMLElement>('aside')!.style.width = width;
      },
    };
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol !== 'ipc:' || url.pathname !== '/update_status') return original.call(window, input, init);
      const state = window.appVersionFixture;
      const response = new Response(
        JSON.stringify(
          state.fail ? { code: 'UPDATE_STATE', message: 'Fixture update status unavailable.' } : state.snapshot,
        ),
        {
          headers: { 'Content-Type': 'application/json', 'Tauri-Response': state.fail ? 'error' : 'ok' },
        },
      );
      if (state.hold) {
        state.hold = false;
        await new Promise<void>((resolve) => {
          state.release = resolve;
        });
      }
      return response;
    };
  });
  // A real native command emits the event; all version/release data is synthetic.
  const changed = () =>
    browser.tauri.execute(async ({ core }) => {
      await core.invoke('update_configure', { mode: 'manual' });
    });
  try {
    await changed();
    await expect($('[aria-label="Remote Codex version"]')).toHaveText('v1.1.1');
    await expect($('button[aria-label^="Update available"]')).not.toExist();
    const update = 'button[aria-label="Update available: v1.2.0"]';
    for (const fail of [false, true]) {
      await browser.execute(() => {
        Object.assign(window.appVersionFixture.snapshot, { available: false, latest: null });
      });
      await changed();
      await expect($(update)).not.toExist();
      await browser.execute((fail) => {
        Object.assign(window.appVersionFixture, { hold: true, fail });
      }, fail);
      await changed();
      await browser.waitUntil(() => browser.execute(() => Boolean(window.appVersionFixture.release)));
      await browser.execute(() => {
        window.appVersionFixture.fail = false;
        Object.assign(window.appVersionFixture.snapshot, {
          available: true,
          latest: { version: '1.2.0', url: 'https://github.com/touale/remote-codex/releases' },
        });
      });
      await changed();
      // The new event must refresh immediately, even while an older request is pending.
      await $(update).waitForDisplayed();
      await browser.executeAsync((done) => {
        window.appVersionFixture.release!();
        delete window.appVersionFixture.release;
        requestAnimationFrame(() => requestAnimationFrame(() => done()));
      });
      await expect($(update)).toBeDisplayed();
      await expect($('[aria-label="Remote Codex version"]')).toHaveAttribute('title', 'Remote Codex v1.1.1');
    }
    await $(update).click();
    await expect($('button=Updates')).toHaveAttribute('aria-current', 'page');
    await expect($('#update-mode')).toBeDisplayed();
    await $('[role=dialog] button[aria-label=Close]').click();
    await $('button=Settings').click();
    await expect($('button=General')).toHaveAttribute('aria-current', 'page');
    await $('[role=dialog] button[aria-label=Close]').click();
    await browser.execute(() => {
      document.querySelector<HTMLElement>('aside')!.style.width = '180px';
    });
    const fits = await browser.execute(() => {
      const aside = document.querySelector('aside')!.getBoundingClientRect();
      const icon = document.querySelector('[aria-label^="Update available"]')!.getBoundingClientRect();
      const version = document.querySelector('[aria-label="Remote Codex version"]')!.getBoundingClientRect();
      const settings = [...document.querySelectorAll('aside button')]
        .find((button) => button.textContent?.trim() === 'Settings')!
        .getBoundingClientRect();
      return icon.right <= aside.right && version.left >= settings.right;
    });
    expect(fits).toBe(true);
    for (const theme of ['light', 'dark']) {
      await browser.execute((theme) => {
        document.documentElement.dataset.theme = theme;
      }, theme);
      await browser.saveScreenshot(
        new URL(`../../../.artifacts/desktop-e2e/app-version-${theme}.png`, import.meta.url).pathname,
      );
    }
    await browser.execute(() => {
      Object.assign(window.appVersionFixture.snapshot, {
        available: false,
        installed_version: '1.2.0',
        restart_required: true,
      });
    });
    await changed();
    await expect($('[aria-label="Remote Codex version"]')).toHaveText('v1.1.1');
    await expect($('button[aria-label="Restart to use v1.2.0"]')).toBeDisplayed();
    await browser.execute(() => {
      window.appVersionFixture.fail = true;
    });
    await $('button[aria-label="Toggle sidebar (⌘B)"]').click();
    await $('button[aria-label="Toggle sidebar (⌘B)"]').click();
    await expect($('[aria-label="Remote Codex version"]')).toHaveText('—');
    await expect($('button[aria-label^="Update available"]')).not.toExist();
    await expect($('button[aria-label^="Restart to use"]')).not.toExist();
  } finally {
    await browser.execute(() => {
      window.appVersionFixture.release?.();
      window.appVersionFixture.restore();
    });
    await browser.tauri.execute(async ({ core }) => {
      await core.invoke('update_configure', { mode: 'notify' });
    });
  }
}
