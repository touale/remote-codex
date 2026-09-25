import { $, browser, expect } from '@wdio/globals';
import { edgeScrollbar } from './scroll-controls';
import { diffContentWindow } from './content-windows';
import { moveDiffView } from './diff-window-transfer';

export async function toolRecords(setTheme: (theme: 'light' | 'dark') => Promise<unknown>) {
  await browser.setWindowSize(1440, 900);
  await $('button=Fixture tools').click();
  await historicalOutput();
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
  await expect($('.editor-pane .diff-heading')).toHaveText(expect.stringContaining('main.rs'));
  await diffContentWindow(setTheme);
  await moveDiffView();
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

async function historicalOutput() {
  await browser.execute(() => {
    const original = window.fetch;
    const offsets: number[] = [];
    const cursors: (string | null)[] = [];
    Object.assign(window, {
      outputOffsets: offsets,
      outputCursors: cursors,
      restoreOutput: () => {
        window.fetch = original;
      },
    });
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol !== 'ipc:' || url.pathname !== '/session_tool_output') return original.call(window, input, init);
      const { offset, source } = JSON.parse(init!.body as string);
      offsets.push(offset);
      cursors.push(source.cursor);
      await new Promise((resolve) => setTimeout(resolve, 200));
      const failed = offsets.length === 2;
      return new Response(
        JSON.stringify(
          failed
            ? { code: 'OUTPUT_FAILED', message: 'Output unavailable. Retry.' }
            : {
                text:
                  offset === 0
                    ? (source.item === 'history-reasoning' ? '**Reasoning**\n\n' : '第一段🙂\n').repeat(80)
                    : '完整结果',
                next_offset: offset === 0 ? 100 : null,
              },
        ),
        { headers: { 'Content-Type': 'application/json', 'Tauri-Response': failed ? 'error' : 'ok' } },
      );
    };
  });
  const offsets = () => browser.execute(() => (window as typeof window & { outputOffsets: number[] }).outputOffsets);
  const row = $('[data-tool-id="history-output"]');
  try {
    expect(await offsets()).toEqual([]);
    await row.$('.tool-heading').click();
    await expect(row.$('[role="status"]')).toHaveText('Loading output…');
    await expect(row.$('pre')).toHaveText('第一段🙂\n'.repeat(80).trim());
    await $('button=Repage tool output').click();
    await expect(row.$('pre')).toHaveText('第一段🙂\n'.repeat(80).trim());
    expect(await offsets()).toEqual([0]);
    await browser.execute(() => {
      const output = document.querySelector<HTMLElement>('[data-tool-id="history-output"] pre')!;
      output.scrollTop = output.scrollHeight;
      output.dispatchEvent(new Event('scroll'));
    });
    await expect(row.$('[role="alert"]')).toHaveText('Output unavailable. Retry.');
    expect(
      await browser.execute(() => (window as typeof window & { outputCursors: (string | null)[] }).outputCursors),
    ).toEqual([null, 'revised-page']);
    await expect(row.$('pre')).toHaveText('第一段🙂\n'.repeat(80).trim());
    await row.$('button=Retry loading output').click();
    await expect(row.$('pre')).toHaveText('第一段🙂\n'.repeat(80) + '完整结果');
    await expect(row.$('button=Load more')).not.toExist();
    expect(await offsets()).toEqual([0, 100, 100]);
    await row.$('.tool-heading').click();
    await row.$('.tool-heading').click();
    await expect(row.$('pre')).toHaveText('第一段🙂\n'.repeat(80).trim());
    expect(await offsets()).toEqual([0, 100, 100, 0]);
    await row.$('.tool-heading').click();
    const reasoning = $('[data-tool-id="history-reasoning"]');
    await reasoning.$('.tool-heading').click();
    await expect(reasoning.$('strong')).toHaveText('Reasoning');
    expect(await offsets()).toEqual([0, 100, 100, 0, 0]);
    await browser.execute(() => {
      const view = document.querySelector<HTMLElement>('[aria-label="Tool records"]')!;
      view.scrollTop = view.scrollHeight;
    });
    await expect(reasoning.$('.tool-reasoning')).toHaveText(expect.stringContaining('完整结果'));
    expect(await offsets()).toEqual([0, 100, 100, 0, 0, 100]);
    await reasoning.$('.tool-heading').click();
  } finally {
    await browser.execute(() => (window as typeof window & { restoreOutput: () => void }).restoreOutput());
  }
}
