import { $, browser, expect } from '@wdio/globals';
interface FirstGate {
  resume?: (error?: string) => void;
  restore: () => void;
  calls: number;
}
declare global {
  interface Window {
    firstMessageGate: FirstGate;
  }
}

// Hold actual IPC setup, including a known pre-submission failure; no extra model turn.
export async function firstMessage() {
  const input = 'textarea[aria-label="Message Codex"]';
  await browser.execute(() => {
    const original = window.fetch;
    const gate: FirstGate = {
      calls: 0,
      restore: () => {
        window.fetch = original;
      },
    };
    window.firstMessageGate = gate;
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol === 'ipc:' && url.pathname === '/session_open') {
        gate.calls++;
        const error = await new Promise<string | undefined>((resolve) => {
          gate.resume = resolve;
        });
        gate.resume = undefined;
        if (error)
          return new Response(JSON.stringify({ code: 'SETUP_INTERRUPTED', message: error, outcome_unknown: false }), {
            headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' },
          });
      }
      return original.call(window, input, init);
    };
  });
  const waiting = async () => {
    await browser.waitUntil(() => browser.execute(() => Boolean(window.firstMessageGate.resume)));
    await expect($('[aria-label="Preparing first message"]')).toBeDisplayed();
    await expect($('[data-pending-submission]')).toHaveText(expect.stringContaining('Check this remote workspace.'));
    await expect($('button[aria-label="Send message"]')).toBeDisabled();
  };
  try {
    await browser.keys('Enter');
    await waiting();
    await expect($(input)).toHaveValue('');
    await $(input).setValue('Next instruction stays here.');
    await browser.keys('Enter');
    expect(await browser.execute(() => window.firstMessageGate.calls)).toBe(1);
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/first-message-loading.png', import.meta.url).pathname,
    );
    await browser.execute(() => window.firstMessageGate.resume?.('Connection interrupted before submission.'));
    await $('.submission-error').waitForDisplayed();
    await expect($(input)).toHaveValue('Next instruction stays here.');
    await $('.submission-error').$('button=Edit').click();
    await $('textarea[aria-label="Unsent message"]').setValue('Check this remote workspace.');
    await $('button=Save message').click();
    await expect($(input)).toHaveValue('Next instruction stays here.');
    await $('.submission-error').$('button=Retry').click();
    await waiting();
    await browser.execute(() => window.firstMessageGate.resume?.());
    await $('[aria-label="Preparing first message"]').waitForExist({ reverse: true, timeout: 60000 });
    await expect($(input)).toHaveValue('Next instruction stays here.');
    await expect($('[data-pending-submission]')).not.toExist();
    await browser.waitUntil(async () => (await $('.messages').getText()).includes('The remote workspace is ready.'));
    expect(
      await browser.execute(
        () =>
          [...document.querySelectorAll('.message.user')].filter((node) =>
            node.textContent?.includes('Check this remote workspace.'),
          ).length,
      ),
    ).toBe(1);
    await $(input).setValue('');
  } finally {
    await browser.execute(() => {
      window.firstMessageGate.restore();
      window.firstMessageGate.resume?.();
    });
  }
}
