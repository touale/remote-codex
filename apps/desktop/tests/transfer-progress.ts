import { $, browser, expect } from '@wdio/globals';

export async function transferProgress() {
  await browser.setWindowSize(1500, 1000);
  await $('button=Fixture transfer progress').click();
  const trigger = $('button[aria-label="File transfers"]');
  const progress = trigger.$('progress');
  try {
    await expect(trigger).toHaveText(expect.stringContaining('Uploading'));
    await expect(progress).toHaveAttribute('value', '25');
    await expect($('[role="dialog"][aria-label="File transfers"]')).not.toExist();
    await $('button=Add download').click();
    await expect(trigger).toHaveText(expect.stringContaining('2 transfers'));
    await expect(progress).toHaveAttribute('value', '43');
    await $('button=Advance transfer').click();
    await expect(progress).toHaveAttribute('value', '50');
    await expect(trigger).toHaveAttribute('title', expect.stringContaining('200.0 MiB / 400.0 MiB'));
    await expect($('#transfer-status-fixture progress[aria-label="Transfer progress"]')).toHaveAttribute('value', '25');
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/transfer-progress.png', import.meta.url).pathname,
    );
    await trigger.click();
    await expect($('[data-transfer-id="upload"]')).toBeDisplayed();
    await expect($('[data-transfer-id="download"]')).toBeDisplayed();
    await $('button[aria-label="Close transfers"]').click();
    await $('button=Narrow status bar').click();
    await browser.setWindowSize(900, 700);
    await expect(progress).toHaveAttribute('value', '50');
    const fits = await browser.execute(() => {
      const bar = document.querySelector('#transfer-status-fixture footer')!;
      const bounds = bar.getBoundingClientRect();
      const button = bar.querySelector('button[aria-label="File transfers"]')!;
      const progress = button.querySelector('progress')!.getBoundingClientRect();
      return (
        bar.scrollWidth <= bounds.width + 1 &&
        progress.width > 0 &&
        progress.left >= bounds.left &&
        progress.right <= bounds.right
      );
    });
    expect(fits).toBe(true);
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/transfer-progress-narrow.png', import.meta.url).pathname,
    );
    await $('button=preparing').click();
    await expect(trigger).toHaveText(expect.stringContaining('Preparing…'));
    await expect(progress).not.toHaveAttribute('value');
    await expect(trigger).not.toHaveText(expect.stringContaining('%'));
    await $('button=reconnecting').click();
    await expect(trigger).toHaveText(expect.stringContaining('Reconnecting…'));
    await expect(progress).toHaveAttribute('value', '50');
    await expect(trigger).not.toHaveAttribute('title', expect.stringContaining('/s'));
    await $('button=paused').click();
    await expect(trigger).toHaveText(expect.stringContaining('Paused'));
    await expect(progress).not.toExist();
    await $('button=conflict').click();
    await expect(trigger).toHaveText(expect.stringContaining('Needs attention'));
    await expect(trigger.$('[aria-label="2 transfers need attention"]')).toBeDisplayed();
    await $('button=completed').click();
    await expect(trigger).toHaveText('Transfers');
    await expect(progress).not.toExist();
  } finally {
    await browser.setWindowSize(1500, 1000);
    await $('button=Fixture transfer progress').click();
  }
}
