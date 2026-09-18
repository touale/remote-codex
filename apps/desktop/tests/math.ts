import { $, $$, browser, expect } from '@wdio/globals';
import path from 'node:path';
import { edgeScrollbar } from './scroll-controls';

export async function mathRendering() {
  await $('button=Fixture math').click();
  await $('#fixture-formulas .katex').waitForDisplayed();
  expect(await $$('#fixture-formulas .katex-display').length).toBe(2);
  expect(await $$('#fixture-formulas .katex-error').length).toBe(0);
  await browser.execute(async () => {
    await document.fonts.ready;
  });
  for (const theme of ['light', 'dark']) {
    await browser.tauri.execute(async ({ core }, theme) => core.invoke('app_preferences', { patch: { theme } }), theme);
    await expect($('html')).toHaveAttribute('data-theme', theme);
    const metrics = await browser.execute(() => {
      const panel = document.querySelector<HTMLElement>('#fixture-wide-math')!;
      const formula = panel.querySelector<HTMLElement>('.katex-display')!;
      const font = document.querySelector<HTMLElement>('#fixture-formulas .katex .mathnormal')!;
      return {
        panelWidth: panel.clientWidth,
        panelScrollWidth: panel.scrollWidth,
        width: formula.clientWidth,
        scrollWidth: formula.scrollWidth,
        font: getComputedStyle(font).fontFamily,
        loaded: [...document.fonts].filter((face) => face.family.startsWith('KaTeX') && face.status === 'loaded')
          .length,
      };
    });
    expect(metrics.panelScrollWidth).toBeLessThanOrEqual(metrics.panelWidth + 1);
    expect(metrics.scrollWidth).toBeGreaterThan(metrics.width);
    expect(metrics.font).toContain('KaTeX');
    expect(metrics.loaded).toBeGreaterThan(0);
    await edgeScrollbar('#fixture-wide-math .katex-display', 'x');
    await browser.saveScreenshot(path.resolve(`../../.artifacts/desktop-e2e/math-${theme}.png`));
  }
  await $('button=Stream formula').click();
  await $('#fixture-stream-math .katex-display').waitForDisplayed();
  await expect($('#fixture-stream-math')).not.toHaveText(expect.stringContaining('RCMATH'));
  expect(await $$('#fixture-stream-math .katex-error').length).toBe(0);
  await $('button=Fixture math').click();
}
