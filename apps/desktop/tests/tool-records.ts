import { $, browser, expect } from '@wdio/globals';
import { edgeScrollbar } from './scroll-controls';

export async function toolRecords(setTheme: (theme: 'light' | 'dark') => Promise<unknown>) {
  await browser.setWindowSize(1000, 800);
  await $('button=Fixture tools').click();
  const row = (id: string) => $(`[data-tool-id="${id}"]`);
  await expect(row('empty').$('.tool-title')).toHaveText('Reasoning summary');
  await expect(row('empty').$('button')).not.toExist();
  await expect(row('empty').$('.tool-chevron')).not.toExist();
  const rightEdges = await browser.execute(() =>
    [...document.querySelectorAll('.tool-status')].map((e) => e.getBoundingClientRect().right),
  );
  expect(Math.max(...rightEdges) - Math.min(...rightEdges)).toBeLessThanOrEqual(1);
  for (const id of ['search', 'mcp', 'reasoning']) {
    const heading = row(id).$('.tool-heading');
    await expect(heading).toHaveAttribute('aria-expanded', 'false');
    await heading.click();
    await expect(heading).toHaveAttribute('aria-expanded', 'true');
    await expect(row(id).$('.tool-details')).toBeDisplayed();
    if (id === 'mcp') await expect(row(id)).toHaveText(expect.stringContaining('Error: Request timed out'));
    if (id === 'reasoning') await expect(row(id).$('strong')).toHaveText('Compare cancellation behavior');
    await heading.click();
    await expect(row(id).$('.tool-details')).not.toExist();
  }
  const command = row('command').$('.tool-heading');
  await command.click();
  await $('button=Finish tool output').click();
  await expect(command).toHaveAttribute('aria-expanded', 'true');
  await expect(row('command')).toHaveText(expect.stringContaining('Finished successfully.'));
  await expect(row('command')).toHaveText(expect.stringContaining('Exit code: 0'));
  await expect(row('command').$('.tool-status')).toHaveText('Done');
  await edgeScrollbar('[data-tool-id="command"] section:last-child pre', 'y');
  await command.click();
  await row('patch').$('.tool-heading').click();
  await row('patch').$('.diff-link').click();
  await expect($('[aria-label="Opened diff"]')).toHaveText(expect.stringContaining('src/main.rs'));
  await row('patch').$('.tool-heading').click();
  await row('search').$('.tool-heading').click();
  // Observe the existing external-link IPC boundary without launching a browser.
  await browser.execute(() => {
    const original = window.fetch;
    Object.assign(window, {
      toolLinkRequests: [],
      restoreToolLinks: () => {
        window.fetch = original;
      },
    });
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol !== 'ipc:' || url.pathname !== '/external_link') return original.call(window, input, init);
      const state = window as typeof window & { toolLinkRequests: string[] };
      state.toolLinkRequests.push(JSON.parse(init!.body as string).url);
      const failed = state.toolLinkRequests.length > 1;
      return new Response(
        JSON.stringify(failed ? { code: 'LINK_FAILED', message: 'Cannot open this link in the browser.' } : null),
        { headers: { 'Content-Type': 'application/json', 'Tauri-Response': failed ? 'error' : 'ok' } },
      );
    };
  });
  try {
    await row('search').$('a').click();
    expect(
      await browser.execute(() => (window as typeof window & { toolLinkRequests: string[] }).toolLinkRequests),
    ).toEqual(['https://doc.rust-lang.org/book/']);
    for (const theme of ['light', 'dark'] as const) {
      await setTheme(theme);
      await expect($('html')).toHaveAttribute('data-theme', theme);
      await browser.saveScreenshot(
        new URL(`../../../.artifacts/desktop-e2e/tool-records-${theme}.png`, import.meta.url).pathname,
      );
    }
    await row('search').$('a').click();
    await expect($('[role="alert"]')).toHaveText('Cannot open this link in the browser.');
  } finally {
    await browser.execute(() => (window as typeof window & { restoreToolLinks: () => void }).restoreToolLinks());
  }
}
