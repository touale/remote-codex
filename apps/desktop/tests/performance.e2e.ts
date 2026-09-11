import { $, browser, expect } from '@wdio/globals';
import { writeFileSync } from 'node:fs';
import path from 'node:path';
it('keeps a long conversation responsive while typing, scrolling, streaming and resizing', async () => {
  await $('section[aria-label="Connections and sessions"]').waitForDisplayed();
  await browser.execute(() => window.dispatchEvent(new Event('remote-codex:performance-fixture')));
  await $('button=Measure long conversation').waitForEnabled({ timeout: 60000 });
  await $('button=Measure long conversation').click();
  await $('[data-performance-result]').waitForExist({ timeout: 120000 });
  const result = JSON.parse(await $('[data-performance-result]').getText());
  writeFileSync(
    path.resolve(
      `../../.artifacts/desktop-e2e/performance-${process.env.REMOTE_CODEX_PERF_BASELINE ? 'before' : 'after'}.json`,
    ),
    JSON.stringify(result, null, 2),
  );
  if (!process.env.REMOTE_CODEX_PERF_BASELINE) {
    for (const name of ['input', 'scroll', 'edge', 'stream', 'transfer', 'resize', 'new_session']) {
      expect(result[name].p95_ms).toBeLessThan(100);
      expect(result[name].median_ms).toBeLessThan(50);
    }
    for (const name of ['input', 'scroll', 'edge', 'stream', 'transfer']) expect(result[name].root_commits).toBe(0);
  }
  await $('button[aria-label="File transfers"]').click();
  await $('[data-transfer-id="performance-transfer"]').waitForDisplayed();
  for (const theme of ['light', 'dark']) {
    await browser.tauri.execute(async ({ core }, theme) => core.invoke('app_preferences', { patch: { theme } }), theme);
    await expect($('html')).toHaveAttribute('data-theme', theme);
    await browser.saveScreenshot(path.resolve(`../../.artifacts/desktop-e2e/transfers-${theme}.png`));
  }
});
