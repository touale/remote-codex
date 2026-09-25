import { $, $$, browser, expect } from '@wdio/globals';
import { chatFollow } from './chat-follow';

export async function historyLoading() {
  await browser.setWindowSize(1100, 800);
  await browser.execute(() => {
    const original = window.fetch;
    const requests: (string | null)[] = [];
    let fail = true;
    Object.assign(window, {
      historyRequests: requests,
      holdHistory: false,
      releaseHistory: () => {},
      restoreHistory: () => {
        window.fetch = original;
      },
    });
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol !== 'ipc:' || url.pathname !== '/session_history') return original.call(window, input, init);
      const { cursor } = JSON.parse(init!.body as string);
      requests.push(cursor);
      const state = window as typeof window & { holdHistory: boolean; releaseHistory: () => void };
      const held = state.holdHistory;
      if (held) {
        state.holdHistory = false;
        await new Promise<void>((resolve) => {
          state.releaseHistory = resolve;
        });
      }
      const failed = cursor === '4' && fail;
      if (failed) fail = false;
      const page = Number(cursor ?? 0);
      return new Response(
        JSON.stringify(
          failed
            ? { code: 'READ_FAILED', message: 'Fixture history failed' }
            : {
                session: {
                  id: 'history-fixture',
                  title: 'History',
                  cwd: '/fixture',
                  created_at: 0,
                  updated_at: 0,
                  archived: false,
                  state: 'idle',
                },
                next_cursor: page < 7 ? String(page + 1) : null,
                turns: [
                  {
                    id: 'turn',
                    status: 'completed',
                    items_before: page < 7,
                    timing: { started_at: 0, completed_at: 1, duration_ms: 1000 },
                    items: Array.from({ length: 16 }, (_, i) => {
                      const index = 128 - (page + 1) * 16 + i;
                      return {
                        id: `${held ? 'stale' : 'message'}-${index}`,
                        kind: 'agentMessage',
                        text: `History entry ${index}`,
                        client_id: null,
                        sent_at: null,
                        phase: null,
                      };
                    }),
                  },
                ],
              },
        ),
        { headers: { 'Content-Type': 'application/json', 'Tauri-Response': failed ? 'error' : 'ok' } },
      );
    };
  });
  const requests = () =>
    browser.execute(() => (window as typeof window & { historyRequests: (string | null)[] }).historyRequests);
  const scrollTop = async () =>
    browser.execute(() => {
      const view = document.querySelector<HTMLElement>('.chat-scroll')!;
      view.scrollTop = 50;
      view.dispatchEvent(new Event('scroll'));
      const first = [...view.querySelectorAll<HTMLElement>('[data-message-id]')].find(
        (item) => item.getBoundingClientRect().bottom > view.getBoundingClientRect().top,
      )!;
      return {
        id: first.dataset.messageId!,
        top: first.getBoundingClientRect().top - view.getBoundingClientRect().top,
      };
    });
  try {
    await $('button=Fixture history').click();
    await browser.waitUntil(async () => (await $$('[data-message-id]').length) === 64);
    expect(await requests()).toEqual([null, '1', '2', '3']);
    await $('button=Toggle conversation').click();
    await $('button=Toggle conversation').click();
    await expect($('[data-message-id="message-64"]')).toExist();
    expect(await requests()).toEqual([null, '1', '2', '3']);
    await expect($('button=Load earlier messages')).not.toExist();
    await chatFollow();
    await scrollTop();
    await expect($('button=Retry loading history')).toBeDisplayed();
    expect(await requests()).toEqual([null, '1', '2', '3', '4']);
    await expect($('[role="alert"]')).toHaveText('Fixture history failed');
    const anchor = await scrollTop();
    // Avoid WebDriver scrolling the partially clipped button into view first.
    await browser.execute(() => document.querySelector<HTMLButtonElement>('button.load-history')!.click());
    await browser.waitUntil(async () => (await $$('[data-message-id]').length) === 96);
    const top = await browser.execute((id) => {
      const view = document.querySelector('.chat-scroll')!;
      return (
        view.querySelector(`[data-message-id="${id}"]`)!.getBoundingClientRect().top - view.getBoundingClientRect().top
      );
    }, anchor.id);
    expect(Math.abs(top - anchor.top)).toBeLessThan(2);
    await browser.execute(() => {
      (window as typeof window & { holdHistory: boolean }).holdHistory = true;
    });
    await scrollTop();
    await browser.waitUntil(async () => (await requests()).at(-1) === '6');
    await $('button=Toggle conversation').click();
    await $('button=Toggle conversation').click();
    await browser.execute(() => (window as typeof window & { releaseHistory: () => void }).releaseHistory());
    await browser.waitUntil(async () => (await $$('[data-message-id]').length) === 128);
    await expect($('[data-message-id^="stale-"]')).not.toExist();
    await scrollTop();
    expect(await requests()).toEqual([null, '1', '2', '3', '4', '4', '5', '6', '6', '7']);
  } finally {
    await $('button=Fixture history').click();
    await browser.execute(() => {
      const fixture = window as typeof window & { releaseHistory: () => void; restoreHistory: () => void };
      fixture.releaseHistory();
      fixture.restoreHistory();
    });
  }
}
