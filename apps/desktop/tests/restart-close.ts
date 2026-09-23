import { $, browser, expect } from '@wdio/globals';
import { withExecuteOptions } from '@wdio/tauri-service';
import { invokeWindow } from './invoke';

declare global {
  interface Window {
    restartFixture: { fail: boolean; restore: () => void };
  }
}

export async function restartCloseGuards() {
  const main = await browser.getWindowHandle();
  const label = await invokeWindow<string>('main', 'new_window', { target: null });
  await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 2);
  const other = (await browser.getWindowHandles()).find((handle) => handle !== main)!;
  try {
    for (const handle of [main, other]) {
      await browser.switchToWindow(handle);
      await $('button=Settings').waitForDisplayed();
      await browser.execute(() => {
        const original = window.fetch;
        window.restartFixture = {
          fail: false,
          restore: () => {
            window.fetch = original;
          },
        };
        window.fetch = async (input, init) => {
          const url = new URL(
            typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
            location.href,
          );
          const command = url.pathname.slice(1);
          if (url.protocol !== 'ipc:' || !['file_read', 'file_write'].includes(command))
            return original.call(window, input, init);
          const failed = command === 'file_write' && window.restartFixture.fail;
          return new Response(
            JSON.stringify(
              failed
                ? { message: 'Fixture save failed.' }
                : {
                    path: 'note.txt',
                    text: command === 'file_write' ? 'unsaved edit' : 'original',
                    revision: 'fixture',
                  },
            ),
            {
              headers: { 'Content-Type': 'application/json', 'Tauri-Response': failed ? 'error' : 'ok' },
            },
          );
        };
        window.dispatchEvent(new Event('remote-codex:controls-fixture'));
      });
      await $('button=Fixture restart').click();
      await $('button=Prepare unsaved file').click();
      await expect($('#restart-dirty')).toHaveText('1');
    }
    await browser.switchToWindow(main);
    expect(await invokeWindow('main', 'restart_app', { force: false })).toEqual({ status: 'closing' });
    expect(await invokeWindow('main', 'restart_app', { force: true })).toEqual({ status: 'closing' });
    await expect(invokeWindow('main', 'new_window', { target: null })).rejects.toMatchObject({
      code: 'APP_RESTARTING',
    });
    await $('h2=Save changes before closing?').waitForDisplayed();
    await $('button=Cancel').click();
    await browser.switchToWindow(other);
    await browser.execute(() => {
      window.restartFixture.fail = true;
    });
    await $('button=Save all').click();
    await expect($('#restart-error')).toHaveText('Fixture save failed.');
    await expect($('#restart-dirty')).toHaveText('1');

    await browser.switchToWindow(main);
    await $('button=Prepare unsent message').click();
    await invokeWindow('main', 'restart_app', { force: true });
    await $('h2=Discard unsent messages?').waitForDisplayed();
    await $('button=Keep editing').click();
    await browser.switchToWindow(other);
    await $('button=Cancel').click();

    await browser.switchToWindow(main);
    await $('button=Clear unsent message').click();
    await browser.execute(() => {
      window.restartFixture.fail = true;
    });
    await invokeWindow('main', 'restart_app', { force: true });
    await $('button=Save all').click();
    await expect($('#restart-error')).toHaveText('Fixture save failed.');
    await browser.switchToWindow(other);
    await browser.execute(() => {
      window.restartFixture.fail = false;
    });
    await $('button=Save all').click();
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 1);
    await browser.switchToWindow(main);
    await expect($('#restart-dirty')).toHaveText('1');
    // A cancellation clears the restart intent: creating windows works again.
    const fresh = await invokeWindow<string>('main', 'new_window', { target: null });
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 2);
    await browser.tauri.execute(
      ({ core }) => {
        void core.invoke('close_window', { cancel: false });
      },
      withExecuteOptions({ windowLabel: fresh }),
    );
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 1);
  } finally {
    if ((await browser.getWindowHandles()).includes(other)) {
      await browser.tauri.execute(
        ({ core }) => {
          void core.invoke('close_window', { cancel: true }).then(() => core.invoke('close_window', { cancel: false }));
        },
        withExecuteOptions({ windowLabel: label }),
      );
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 1);
    }
    await browser.switchToWindow(main);
    await browser.execute(() => window.restartFixture?.restore());
    await browser.refresh();
    await $('button=Settings').waitForDisplayed();
  }
}
