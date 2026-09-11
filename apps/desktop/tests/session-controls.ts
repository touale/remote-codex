import { $, browser, expect } from '@wdio/globals';
import { writeFileSync } from 'node:fs';
import type { HistoryPage, SessionOpened, SessionSnapshot, TextFile } from '../src/bridge/types';
import { rowMenu } from './interaction-controls';
import { newSession } from './navigation-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export async function sessionControls(invoke: Invoke, session: string, workspace: string, remote: string) {
  const choose = async (mode: 'Code' | 'Plan' | 'Goal') => {
    await $('button[aria-label="Session mode"]').waitForEnabled();
    await $('button[aria-label="Session mode"]').click();
    await $('.mode-menu').$(`button*=${mode}`).click();
    await expect($('button[aria-label="Session mode"]')).toHaveText(expect.stringContaining(mode));
  };
  await choose('Plan');
  await browser.waitUntil(
    async () => (await invoke<SessionSnapshot>('session_snapshot', { id: session })).settings.mode === 'plan',
  );
  await $('textarea[aria-label="Message Codex"]').setValue('Make a short plan for this workspace.');
  await $('button[aria-label="Send message"]').click();
  await $('button=Implement plan').waitForDisplayed({ timeout: 30000 });
  await $('.working').waitForExist({ reverse: true });
  await $('button=Implement plan').click();
  await expect($('.messages')).toHaveText(expect.stringContaining('Implemented the selected workspace plan.'));
  await $('.working').waitForExist({ reverse: true });
  expect((await invoke<SessionSnapshot>('session_snapshot', { id: session })).settings.mode).toBe('agent');
  await choose('Goal');
  await $('textarea[aria-label="Message Codex"]').setValue(
    'Create the isolated remote Goal marker, then complete the goal.',
  );
  await $('button[aria-label="Start goal"]').click();
  await $('.question').waitForDisplayed({ timeout: 30000 });
  await $('.question button.primary').click();
  await expect($('.messages')).toHaveText(expect.stringContaining('The workspace goal is complete.'));
  await $('.working').waitForExist({ reverse: true });
  const final = await invoke<SessionSnapshot>('session_snapshot', { id: session });
  expect(final.goal?.status).toBe('complete');
  expect(final.goal!.tokens_used).toBeGreaterThan(0);
  expect(final.settings.model).toBe('gpt-5.4-mini');
  expect((await invoke<TextFile>('file_read', { context: workspace, path: 'goal.txt' })).text).toBe('goal-ok\n');

  const history = await invoke<HistoryPage>('session_history', { id: session, cursor: null });
  writeFileSync(new URL('../../../.artifacts/desktop-e2e/history.json', import.meta.url), JSON.stringify(history));
  expect(history.turns.some((turn) => turn.timing.completed_at !== null && turn.timing.duration_ms !== null)).toBe(
    true,
  );
  expect(history.turns.flatMap((turn) => turn.items).filter((item) => item.client_id).length).toBeGreaterThanOrEqual(5);
  expect(
    history.turns
      .flatMap((turn) => turn.items)
      .filter((item) => item.client_id)
      .every((item) => item.sent_at !== null),
  ).toBe(true);
  await expect($('.turn-footer time')).toExist();
  await expect($('.message.user .message-time')).toExist();
  await $('textarea[aria-label="Message Codex"]').setValue('/status');
  await browser.keys('Enter');
  await expect($('.session-status')).toHaveText(expect.stringContaining(session));
  await expect($('button[aria-label="Refresh session status"]')).toBeEnabled();
  await expect($('.session-status .inline-error')).not.toExist();
  expect((await invoke<SessionSnapshot>('session_status', { id: session })).status.limits_error).toBeNull();
  await expect($('.session-status')).toHaveText(expect.stringContaining('Session total'));
  await expect($('.session-status')).toHaveText(expect.stringContaining('gpt-5.4-mini'));
  await browser.saveScreenshot(new URL('../../../.artifacts/desktop-e2e/session-status.png', import.meta.url).pathname);
  await $('[role="dialog"] button[aria-label="Close"]').click();
  const beforeRestore = await $('.message-time').getAttribute('datetime');
  // A → B → A keeps the existing owner and unsent draft.
  await $('textarea[aria-label="Message Codex"]').setValue('Keep this draft.');
  expect(
    await browser.execute(() => {
      const view = document.querySelector('.chat-scroll')!;
      view.scrollTop = 0;
      view.dispatchEvent(new Event('scroll', { bubbles: true }));
      return view.scrollHeight - view.clientHeight > 100;
    }),
  ).toBe(true);
  await newSession();
  await browser.waitUntil(async () => (await $('footer').getAttribute('data-session-id')) !== session);
  await $(`[data-session-id="${session}"] .tree-label`).click();
  await expect($('textarea[aria-label="Message Codex"]')).toHaveValue('Keep this draft.');
  expect(await browser.execute(() => document.querySelector('.chat-scroll')!.scrollTop)).toBe(0);
  await expect($('[role="dialog"]')).not.toExist();
  // Concurrent direct callers must also share one native owner.
  await browser.execute(
    (id) =>
      document
        .querySelector(`[data-session-id="${id}"]`)!
        .dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 100, clientY: 220 })),
    session,
  );
  await $('[role="menuitem"]=Close session').click();
  await $('button=Resume session').waitForDisplayed();
  await rowMenu(`[data-session-id="${session}"]`, 'Open session', 'Close session');
  const responses = await Promise.all(
    [0, 1].map(() =>
      invoke<SessionOpened>('session_open', {
        operationId: crypto.randomUUID(),
        input: { server: 'desktop-test', path: remote, resume: session, takeover: false, mcp_source: null },
      }),
    ),
  );
  expect(responses.every((value) => value.status === 'open' && value.session.id === session)).toBe(true);
  for (const value of responses) {
    if (value.status !== 'open') throw new Error('Session did not reopen');
    expect(value.snapshot.status.usage).toEqual(final.status.usage);
    expect(value.snapshot.status.usage?.total_tokens).toBeGreaterThan(0);
  }
  await $(`[data-session-id="${session}"] .tree-label`).click();
  await expect($('textarea[aria-label="Message Codex"]')).toBeDisplayed();
  expect((await invoke<SessionSnapshot>('session_snapshot', { id: session })).goal?.status).toBe('complete');
  await browser.waitUntil(async () => (await $('.message-time').getAttribute('datetime')) === beforeRestore);
}
