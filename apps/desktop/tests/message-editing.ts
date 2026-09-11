import { $, $$, browser, expect } from '@wdio/globals';
import type { Catalog, HistoryPage, SessionSnapshot } from '../src/bridge/types';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function messageEditing(invoke: Invoke, id: string) {
  await $('button[aria-label="Session mode"]').click();
  await $('.mode-menu').$('button*=Plan').click();
  await $('textarea[aria-label="Message Codex"]').setValue('Clarify this plan.');
  await $('button[aria-label="Send message"]').click();
  await $('.async-questions').waitForDisplayed({ timeout: 30000 });
  await expect($$('.async-questions input[type="radio"]')).toBeElementsArrayOfSize(4);
  await browser.saveScreenshot(new URL('../../../.artifacts/desktop-e2e/plan-questions.png', import.meta.url).pathname);
  await browser.waitUntil(async () => !(await invoke<SessionSnapshot>('session_snapshot', { id })).turn);
  await $('.async-questions .question-option:nth-of-type(2)').click();
  await $('.async-questions').$('button=Send answers').click();
  await expect($('.messages')).toHaveText(expect.stringContaining('The choices are recorded.'));
  await expect($('.async-questions')).toHaveText(expect.stringContaining('Answered'));
  await browser.waitUntil(async () => !(await invoke<SessionSnapshot>('session_snapshot', { id })).turn);
  const before = await invoke<HistoryPage>('session_history', { id });
  const target = before.turns.find((t) => t.items.some((i) => i.text === 'Clarify this plan.'))!;
  expect(target.items.find((i) => i.delivery === 'async')?.questions).toHaveLength(2);
  expect(before.turns.some((t) => t.items.some((i) => i.client_id?.startsWith('question:')))).toBe(true);
  const draft = 'Keep this unrelated composer draft.';
  await $('textarea[aria-label="Message Codex"]').setValue(draft);
  const message = $(`[data-turn-id="${target.id}"] .message.user`);
  await message.moveTo();
  await message.$('button[aria-label="Edit message"]').click();
  await $('textarea[aria-label="Edit message"]').setValue('Clarify the revised plan.');
  await browser.saveScreenshot(
    new URL('../../../.artifacts/desktop-e2e/message-editing.png', import.meta.url).pathname,
  );
  await $('button=Save & resend').click();
  await expect($('.messages')).toHaveText(expect.stringContaining('The revised question has been processed.'));
  await expect($('.messages')).not.toHaveText(expect.stringContaining('The choices are recorded.'));
  await expect($('.async-questions')).not.toExist();
  await expect($('textarea[aria-label="Message Codex"]')).toHaveValue(draft);
  expect(await $('footer').getAttribute('data-session-id')).toBe(id);
  const after = await invoke<HistoryPage>('session_history', { id });
  expect(after.turns.some((t) => t.id === target.id)).toBe(false);
  expect(after.turns.some((t) => t.items.some((i) => i.text === 'Clarify the revised plan.'))).toBe(true);
  await browser.waitUntil(async () => !(await invoke<SessionSnapshot>('session_snapshot', { id })).turn);
  const first = after.turns.find((t) => t.items.some((i) => i.text === 'Check this remote workspace.'))!;
  const firstMessage = $(`[data-turn-id="${first.id}"] .message.user`);
  await firstMessage.moveTo();
  await firstMessage.$('button[aria-label="Edit message"]').click();
  await $('textarea[aria-label="Edit message"]').setValue('Recheck this remote workspace.');
  await $('button=Save & resend').click();
  await expect($('.messages')).toHaveText(expect.stringContaining('The first message was regenerated.'));
  await expect($('.messages')).not.toHaveText(expect.stringContaining('Clarify the revised plan.'));
  await expect($('textarea[aria-label="Message Codex"]')).toHaveValue(draft);
  expect((await invoke<HistoryPage>('session_history', { id })).turns).toHaveLength(1);
}

export async function sessionTitle(invoke: Invoke, id: string) {
  await expect($(`[data-session-id="${id}"] .tree-label`)).toHaveText(
    expect.stringContaining('Check this remote workspace.'),
  );
  const catalog = await invoke<Catalog>('catalog', { archived: false });
  expect(catalog.sessions.find((s) => s.session.id === id)?.session.title).toBe('Check this remote workspace.');
}
