import { $, browser, expect } from '@wdio/globals';
import { writeFileSync } from 'node:fs';
import type { Catalog, SessionSnapshot } from '../src/bridge/types';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function immediateDraft(invoke: Invoke) {
  const before = await invoke<Catalog>('catalog', { archived: false });
  const samples = await browser.execute(async () => {
    const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    const samples: number[] = [];
    for (let n = 0; n < 12; n++) {
      await frame();
      const path = document.querySelector('.workspace-path')!.getAttribute('title')!;
      const target = document.querySelector(`[data-workspace-path="${CSS.escape(path)}"] .tree-label`)!;
      const bounds = target.getBoundingClientRect();
      target.dispatchEvent(
        new MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: bounds.left + 8,
          clientY: bounds.top + 8,
        }),
      );
      await frame();
      await frame();
      const started = performance.now();
      [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')]
        .find((item) => item.textContent === 'New session')!
        .click();
      await frame();
      await frame();
      if (!document.querySelector('section[aria-label="New conversation"] textarea'))
        throw new Error('Draft was not ready');
      samples.push(performance.now() - started);
    }
    return samples;
  });
  writeFileSync(
    new URL('../../../.artifacts/desktop-e2e/new-session-performance.json', import.meta.url),
    JSON.stringify(samples),
  );
  expect(Math.max(...samples)).toBeLessThan(100);
  const after = await invoke<Catalog>('catalog', { archived: false });
  expect(after.live.length).toBe(before.live.length);
  expect(after.sessions.length).toBe(before.sessions.length);
  await expect($('[role=dialog]')).not.toExist();
  await expect($('.chat')).not.toHaveText(expect.stringContaining('Your environment connects when you send.'));
  await $('button[aria-label="Model and reasoning"]').waitForEnabled({ timeout: 30000 });
  await expect($('button[aria-label="Model and reasoning"]')).not.toHaveText(expect.stringContaining('default'));

  await expect($('.usage-strip')).toHaveText(expect.stringContaining('5h'));
  await expect($('.usage-strip')).toHaveText(expect.stringContaining('Context'));
  await expect($('.usage-strip')).not.toHaveText(expect.stringContaining('Weekly'));
}

export async function draftModes(invoke: Invoke, original: string) {
  for (const mode of ['Plan', 'Goal']) {
    await immediateDraft(invoke);
    await $('button[aria-label="Session mode"]').click();
    await $('.mode-menu').$(`button*=${mode}`).click();
    await $('button[aria-label="Model and reasoning"]').click();
    await $('button=Other model…').click();
    await $('input[aria-label="Custom model ID"]').setValue('gpt-5.4-mini');
    await $('button=Use model').click();
    await $('textarea[aria-label="Message Codex"]').setValue(
      mode === 'Plan' ? 'Plan the next task.' : 'Complete this fresh goal.',
    );
    await $(`button[aria-label="${mode === 'Plan' ? 'Send message' : 'Start goal'}"]`).click();
    const text = mode === 'Plan' ? 'A plan from a fresh draft.' : 'A fresh goal is complete.';
    await browser.waitUntil(async () => (await $('.messages').getText()).includes(text), { timeout: 45000 });
    const id = await $('footer').getAttribute('data-session-id');
    const state = await invoke<SessionSnapshot>('session_snapshot', { id });
    expect(state.settings.model).toBe('gpt-5.4-mini');
    if (mode === 'Plan') expect(state.settings.mode).toBe('plan');
    else expect(state.goal?.status).toBe('complete');
    await invoke('session_close', { id });
  }
  await $(`[data-session-id="${original}"] .tree-label`).click();
}

export async function usagePreferences() {
  const settings = async () => {
    await $('button=Settings').click();
  };
  const close = async () => {
    await $('[role=dialog] button[aria-label=Close]').click();
  };
  await settings();
  await $('#setting-weekly_limit').click();
  await $('#setting-session_tokens').click();
  await close();
  await expect($('.usage-strip')).toHaveText(expect.stringContaining('Weekly'));
  await expect($('.usage-strip')).toHaveText(expect.stringContaining('Tokens'));
  await settings();
  for (const key of ['five_hour_limit', 'weekly_limit', 'context_usage', 'session_tokens'])
    await $(`#setting-${key}`).click();
  await close();
  await expect($('.usage-strip')).not.toExist();
  await settings();
  await $('#setting-five_hour_limit').click();
  await $('#setting-context_usage').click();
  await close();
  await expect($('.usage-strip')).toHaveText(expect.stringContaining('Context'));
}
