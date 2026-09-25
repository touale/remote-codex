import { $, browser, expect } from '@wdio/globals';
import { activateTree } from './interaction-controls';
import { newSession } from './navigation-controls';

type Stage = 'opening' | 'history';
interface LoadingGate {
  counts: Record<Stage, number>;
  pending: Partial<Record<Stage, { resolve: () => void; reject: (error: unknown) => void }>>;
  restore: () => void;
}
declare global {
  interface Window {
    conversationLoadingGate: LoadingGate;
  }
}

// Gate native reads at the test IPC boundary; production has no loading delays or test hooks.
export async function conversationLoading(session: string) {
  const workspacePath = await $('.workspace-path').getText();
  const row = `[data-session-id="${session}"] .tree-label`;
  const input = 'textarea[aria-label="Message Codex"]';
  const loading = '[aria-label="Conversation"][aria-busy="true"]';
  const alerts = () =>
    browser.execute(() =>
      [...document.querySelectorAll('[role="alert"]')].map((node) => node.textContent?.trim()).filter(Boolean),
    );
  const settle = (stage: Stage, code?: string) =>
    browser.execute(
      (stage, code) => {
        const pending = window.conversationLoadingGate.pending[stage]!;
        delete window.conversationLoadingGate.pending[stage];
        if (code)
          pending.reject({ code, message: 'History read interrupted. Please try again.', outcome_unknown: false });
        else pending.resolve();
      },
      stage,
      code,
    );
  const waitFor = async (stage: Stage) => {
    await browser.waitUntil(() =>
      browser.execute((stage) => Boolean(window.conversationLoadingGate.pending[stage]), stage),
    );
    await expect($(loading)).toBeDisplayed();
    await expect($('[aria-label="Conversation"]')).toHaveAttribute('data-loading-phase', stage);
    await expect($(input)).not.toExist();
    await expect($(row)).toHaveAttribute('aria-selected', 'true');
  };
  await browser.execute(
    (id) =>
      document
        .querySelector(`[data-session-id="${id}"]`)!
        .dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 100, clientY: 220 })),
    session,
  );
  await $('[role="menuitem"]=Close session').click();
  await $('button=Resume session').waitForDisplayed();
  await newSession();
  await $(input).setValue('Preserve this unsent draft.');
  await browser.execute((session) => {
    const original = window.fetch;
    const gate: LoadingGate = {
      counts: { opening: 0, history: 0 },
      pending: {},
      restore: () => {
        window.fetch = original;
      },
    };
    window.conversationLoadingGate = gate;
    // Tauri's macOS invoke properties are immutable. Delay only these IPC fetches
    // and return typed failures as responses, preserving its normal callback path.
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      const stage =
        url.protocol === 'ipc:'
          ? ({ '/session_open': 'opening', '/session_history': 'history' } as const)[
              url.pathname as '/session_open' | '/session_history'
            ]
          : undefined;
      const args = stage && typeof init?.body === 'string' ? JSON.parse(init.body) : null;
      if (stage && (args?.id === session || args?.input?.resume === session)) {
        gate.counts[stage]++;
        try {
          await new Promise<void>((resolve, reject) => {
            gate.pending[stage] = { resolve, reject };
          });
        } catch (error) {
          return new Response(JSON.stringify(error), {
            headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' },
          });
        }
      }
      return original.call(window, input, init);
    };
  }, session);
  try {
    await $(row).click();
    await waitFor('opening');
    await expect($('[aria-label="Conversation heading"]')).toHaveText(expect.stringContaining('desktop-test'));
    await settle('opening');
    await waitFor('history');
    await expect($('footer')).toHaveText(expect.stringContaining('Loading conversation history…'));
    // Repeated selection shares the native open and the initial history request.
    await $(row).click();
    expect(await browser.execute(() => window.conversationLoadingGate.counts)).toEqual({ opening: 1, history: 1 });
    await $('button[aria-label="Toggle editor"]').click();
    await $('button[aria-label="Minimize terminal"]').click();
    for (const theme of ['Light', 'Dark']) {
      await $('button=Settings').click();
      await $(`button=${theme}`).click();
      await $('[role="dialog"] button[aria-label="Close"]').click();
      await browser.saveScreenshot(
        new URL(`../../../.artifacts/desktop-e2e/conversation-loading-${theme.toLowerCase()}.png`, import.meta.url)
          .pathname,
      );
    }
    await browser.setWindowSize(900, 600);
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/conversation-loading-minimum.png', import.meta.url).pathname,
    );
    await browser.setWindowSize(1280, 800);
    // Leaving while history is pending keeps the draft; a stale failure must not replace it.
    await newSession();
    await expect($(input)).toHaveValue('Preserve this unsent draft.');
    await settle('history', 'HISTORY_UNAVAILABLE');
    await expect($(loading)).not.toExist();
    expect(await alerts()).toEqual([]);

    await $(row).click();
    await waitFor('history');
    await settle('history', 'HISTORY_UNAVAILABLE');
    await expect($('[aria-label="Conversation"] [role="alert"]')).toHaveText(
      'History read interrupted. Please try again.',
    );
    await expect($(loading)).not.toExist();
    await expect($(input)).toBeDisplayed();
    await newSession();
    await expect($(input)).toHaveValue('Preserve this unsent draft.');

    await $(row).click();
    await waitFor('history');
    await settle('history', 'OPERATION_CANCELLED');
    await expect($(input)).toHaveValue('Preserve this unsent draft.');
    expect(await alerts()).toEqual([]);

    await $(row).click();
    await waitFor('history');
    await settle('history', 'HISTORY_UNAVAILABLE');
    await $('button=Retry loading history').waitForDisplayed();
    await $('button=Retry loading history').click();
    await browser.waitUntil(() => browser.execute(() => Boolean(window.conversationLoadingGate.pending.history)));
    await expect($(input)).toBeDisplayed();
    // A successful late response must not take over the selected server home.
    await activateTree('.server-row .tree-label');
    await settle('history');
    await expect($('[aria-label="Server home"]')).toBeDisplayed();
    await expect($(input)).not.toExist();
    await $('[aria-label="Server home"] button[aria-label="New session"]').click();
    await $('[role="dialog"]').$(`span=desktop-test · ${workspacePath}`).click();
    await expect($(input)).toHaveValue('Preserve this unsent draft.');
    await $(row).click();
    await expect($(input)).toHaveValue('Keep this draft.');
    await expect($('.messages')).toHaveText(expect.stringContaining('The workspace goal is complete.'));
    await expect($(loading)).not.toExist();
    const counts = await browser.execute(() => window.conversationLoadingGate.counts);
    expect(counts).toEqual({ opening: 1, history: 5 });

    await newSession();
    await expect($(input)).toHaveValue('Preserve this unsent draft.');
    await $(input).setValue('');
    await $(row).click();
    await expect($(input)).toHaveValue('Keep this draft.');
    expect(await browser.execute(() => document.querySelector('.chat-scroll')!.scrollTop)).toBe(0);
    expect(await browser.execute(() => window.conversationLoadingGate.counts)).toEqual(counts);
    await expect($(loading)).not.toExist();
  } catch (error) {
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/conversation-loading-failure.png', import.meta.url).pathname,
    );
    throw error;
  } finally {
    await browser.execute(() => {
      window.conversationLoadingGate.restore();
      for (const pending of Object.values(window.conversationLoadingGate.pending)) pending?.resolve();
    });
  }
}
