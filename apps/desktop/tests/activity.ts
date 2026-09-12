import { $, browser, expect } from '@wdio/globals';
import path from 'node:path';

export async function activityTimes(setTheme: (theme: 'light' | 'dark') => Promise<unknown>) {
  await browser.setWindowSize(1100, 800);
  await $('button=Fixture activity').click();
  const row = (id: string) => $(`#activity-tree [data-session-id="${id}"]`);
  await expect(row('old').$('time')).toHaveText('3d');
  await expect(row('minutes').$('time')).toHaveText('2m');
  await expect(row('unknown').$('time')).toHaveText('—');
  await expect(row('recent').$('time')).toHaveAttribute('title', expect.stringContaining('Last active:'));
  const order = () =>
    browser.execute(() =>
      [...document.querySelectorAll('#activity-tree [data-session-id]')].map((e) => e.getAttribute('data-session-id')),
    );
  expect(await order()).toEqual(['recent', 'minutes', 'old', 'unknown']);
  for (const theme of ['light', 'dark'] as const) {
    await setTheme(theme);
    const layout = await browser.execute(() =>
      [...document.querySelectorAll('#activity-tree .session-row')].map((row) => {
        const time = row.querySelector('time')!;
        const title = row.querySelector('.tree-label > span')!;
        const box = time.getBoundingClientRect();
        return {
          right: box.right,
          inside: box.left >= row.getBoundingClientRect().left && box.right <= row.getBoundingClientRect().right,
          ellipsis: title.scrollWidth > title.clientWidth && getComputedStyle(title).textOverflow === 'ellipsis',
        };
      }),
    );
    expect(layout.every((item) => item.inside && item.ellipsis)).toBe(true);
    expect(Math.max(...layout.map((r) => r.right)) - Math.min(...layout.map((r) => r.right))).toBeLessThanOrEqual(1);
    await browser.saveScreenshot(path.resolve(`../../.artifacts/desktop-e2e/activity-${theme}.png`));
  }
  await $('button=Activity boundary').click();
  await browser.waitUntil(async () => (await row('recent').$('time').getText()) === '1m', { timeout: 5000 });
  await $('button=Update activity').click();
  await browser.waitUntil(async () => (await order())[0] === 'old');
  await expect($('[aria-label="Recent sessions"] time')).toHaveText(/^\d+s$/);
  await $('button=Fixture activity').click();
}
