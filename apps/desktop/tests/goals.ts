import { $, browser, expect } from '@wdio/globals';
import type { SessionAction } from '../src/bridge/session';

export async function goalControls() {
  await $('button=Fixture goals').click();
  const input = $('textarea[aria-label="Message Codex"]');
  const mode = $('button[aria-label="Session mode"]');
  const actions = async (): Promise<SessionAction[]> => JSON.parse(await $('#goal-actions').getText());
  try {
    await mode.click();
    await $('.mode-menu').$('button*=Goal').click();
    await input.setValue('Keep working on this goal.');
    await $('button=Reject next goal request').click();
    await $('button[aria-label="Start goal"]').click();
    await expect($('.composer-wrap [role="alert"]')).toHaveText('Fixture goal request failed.');
    await expect(mode).toHaveText(expect.stringContaining('Goal'));
    await expect(input).toHaveValue('Keep working on this goal.');
    await $('button[aria-label="Start goal"]').click();
    await expect(mode).toHaveText(expect.stringContaining('Code'));
    await expect(input).toHaveValue('');
    await expect($('[aria-label="Session goal"]')).toHaveText(expect.stringContaining('Working toward goal'));

    await $('button=Disconnect goal').click();
    await expect($('button[aria-label="Resume goal"]')).toBeDisabled();
    await $('button=Restore goal connection').click();
    await expect($('.working')).toExist();
    await expect($('button[aria-label="Resume goal"]')).toBeEnabled();
    await $('button=Reject next goal request').click();
    await $('button[aria-label="Resume goal"]').click();
    await expect($('.composer-wrap [role="alert"]')).toHaveText('Fixture goal request failed.');
    await expect($('button[aria-label="Resume goal"]')).toBeEnabled();
    await $('button[aria-label="Resume goal"]').click();
    await expect($('[aria-label="Session goal"]')).toHaveText(expect.stringContaining('Working toward goal'));
    await expect(mode).toHaveText(expect.stringContaining('Code'));

    await $('button=Complete goal').click();
    await input.setValue('Explain the result.');
    await $('button[aria-label="Send message"]').click();
    await browser.waitUntil(async () => (await actions()).at(-1)?.action === 'submit');
    await $('button=Start draft goal').click();
    await expect($('[aria-label="Session goal"]')).toHaveText(expect.stringContaining('A fresh goal.'));
    await expect(mode).toHaveText(expect.stringContaining('Code'));
    const sent = await actions();
    expect(sent.filter((action) => action.action === 'goal').map((action) => action.goal.action)).toEqual([
      'set',
      'set',
      'resume',
      'resume',
      'set',
    ]);
  } finally {
    await $('button=Fixture goals').click();
  }
}
