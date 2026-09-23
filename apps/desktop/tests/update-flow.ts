import { $, browser, expect } from '@wdio/globals';
import type { UpdateSnapshot } from '../src/bridge/updates';

declare global {
  interface Window {
    updateFlow: {
      snapshot: UpdateSnapshot;
      fail: string;
      defer: boolean;
      release?: () => void;
      installs: number;
      restarts: boolean[];
      restore: () => void;
    };
  }
}

export async function updateFlow() {
  await browser.execute(() => {
    const original = window.fetch;
    window.updateFlow = {
      snapshot: {
        current_version: '1.1.1',
        installed_version: '1.1.1',
        mode: 'manual',
        last_checked: null,
        latest: { version: '1.2.0', url: 'https://example.com/releases' },
        path: '/fixture/Remote Codex.app',
        can_install: true,
        busy: false,
        available: false,
        restart_required: false,
      },
      fail: '',
      defer: false,
      installs: 0,
      restarts: [],
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
      if (
        url.protocol !== 'ipc:' ||
        !['update_status', 'update_check', 'update_install', 'restart_app'].includes(command)
      )
        return original.call(window, input, init);
      const state = window.updateFlow;
      let result: unknown;
      if (command === 'update_install') {
        state.installs++;
        if (state.defer)
          await new Promise<void>((resolve) => {
            state.release = resolve;
          });
        if (state.fail !== command)
          Object.assign(state.snapshot, { installed_version: '1.2.0', available: false, restart_required: true });
      }
      if (command === 'restart_app') {
        const { force } = JSON.parse(init!.body as string);
        state.restarts.push(force);
        result = force
          ? { status: 'closing' }
          : { status: 'confirmation_required', sessions: 2, terminals: 1, transfers: 1 };
      }
      const failed = state.fail === command;
      return new Response(JSON.stringify(failed ? { message: 'Fixture update failed.' } : (result ?? state.snapshot)), {
        headers: { 'Content-Type': 'application/json', 'Tauri-Response': failed ? 'error' : 'ok' },
      });
    };
  });
  const check = () => $('button=Check for Updates').click();
  const prompt = (title: string) => $(`h2=${title}`);
  try {
    await $('button=Settings').click();
    await $('button=Updates').click();
    await $('button=Check for Updates').waitForEnabled();
    await check();
    await expect($('.update-actions')).toHaveText('The app is up to date.');
    await browser.execute(() => {
      window.updateFlow.fail = 'update_check';
    });
    await check();
    await expect($('[role="alert"]')).toHaveText('Fixture update failed.');
    await browser.execute(() => {
      window.updateFlow.fail = '';
      window.updateFlow.snapshot.available = true;
    });
    await check();
    await prompt('Update available').waitForDisplayed();
    await expect($('button=Check for Updates')).toBeDisabled();
    await expect($('button=Checking…')).not.toExist();
    await expect($('.settings-modal .spinning')).not.toExist();
    await expect($('.settings-modal button[aria-label="Close"]')).toBeDisabled();
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/update-available.png', import.meta.url).pathname,
    );
    await $('button=Later').click();
    expect(await browser.execute(() => window.updateFlow.installs)).toBe(0);

    await browser.execute(() => {
      window.updateFlow.fail = 'update_install';
    });
    await check();
    await $('button=Update now').click();
    await expect($('[role="alert"]')).toHaveText('Fixture update failed.');
    await expect(prompt('Restart to update?')).not.toExist();

    await browser.execute(() => {
      window.updateFlow.fail = '';
      window.updateFlow.defer = true;
    });
    await $('button=Update App').click();
    await browser.waitUntil(() => browser.execute(() => Boolean(window.updateFlow.release)));
    await expect($('button=Updating…')).toBeDisabled();
    await expect($('button=General')).toBeDisabled();
    await expect($('.settings-modal button[aria-label="Close"]')).toBeDisabled();
    await browser.keys('Escape');
    await expect($('#update-mode')).toBeDisplayed();
    await browser.execute(() => window.updateFlow.release!());
    await prompt('Restart to update?').waitForDisplayed();
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/update-restart.png', import.meta.url).pathname,
    );
    await $('button=Later').click();
    await expect($('button=Restart now')).toBeEnabled();
    await expect($('.settings-modal button[aria-label="Close"]')).toBeEnabled();
    await expect($('small=App version 1.1.1')).toBeDisplayed();
    expect(await browser.execute(() => window.updateFlow.restarts)).toEqual([]);
    await $('button=Restart now').click();
    await prompt('Tasks are still running').waitForDisplayed();
    await expect($('p*=2 active sessions, 1 open terminal, and 1 file transfer')).toBeDisplayed();
    await $('button=Cancel').click();
    expect(await browser.execute(() => window.updateFlow.restarts)).toEqual([false]);
    await $('button=Restart now').click();
    await $('button=Restart anyway').click();
    await browser.waitUntil(() => browser.execute(() => window.updateFlow.restarts.length === 3));
    expect(await browser.execute(() => window.updateFlow.restarts)).toEqual([false, false, true]);
    expect(await browser.execute(() => window.updateFlow.installs)).toBe(2);
    await $('[role="dialog"] button[aria-label="Close"]').click();
  } finally {
    await browser.execute(() => {
      window.updateFlow.release?.();
      window.updateFlow.restore();
    });
  }
}
