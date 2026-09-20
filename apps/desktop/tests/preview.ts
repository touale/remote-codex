import { $, browser, expect } from '@wdio/globals';
import { previewPdf, previewSvg, previewMarkdown } from './preview-data';

declare global {
  interface Window {
    previewFixture: { reads: string[]; cancel: number; writes: string[]; restore: () => void; release?: () => void };
  }
}
export async function filePreviews() {
  await browser.setWindowSize(1440, 1000);
  await browser.execute(
    (pdf, svg, markdown) => {
      const original = window.fetch;
      let retryRead = false;
      const state = (window.previewFixture = {
        reads: [],
        cancel: 0,
        writes: [],
        restore: () => {
          window.fetch = original;
        },
      } as Window['previewFixture']);
      window.fetch = async (input, init) => {
        const url = new URL(
          typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
          location.href,
        );
        if (
          url.protocol !== 'ipc:' ||
          !['/file_read', '/file_preview_read', '/file_write', '/cancel_operation', '/file_context_open'].includes(
            url.pathname,
          )
        )
          return original.call(window, input, init);
        const args = JSON.parse(init!.body as string);
        const headers = { 'Content-Type': 'application/json', 'Tauri-Response': 'ok' };
        if (url.pathname === '/cancel_operation') {
          state.cancel++;
          state.release?.();
          return new Response('null', { headers });
        }
        if (url.pathname === '/file_context_open')
          return new Response(
            JSON.stringify({ id: 'preview-fixture', server: 'fixture', path: '/project', kind: 'workspace' }),
            { headers },
          );
        if (url.pathname === '/file_read' && args.path === 'retry.md' && !retryRead) {
          retryRead = true;
          return new Response(JSON.stringify({ code: 'READ_FAILED', message: 'Temporary read failure' }), {
            headers: { ...headers, 'Tauri-Response': 'error' },
          });
        }
        if (url.pathname === '/file_write') {
          state.writes.push(args.text);
          return new Response('"saved"', { headers });
        }
        if (url.pathname === '/file_read')
          return new Response(
            JSON.stringify({
              path: args.path,
              text: args.path === 'notes/readme.md' ? markdown : '# Next document',
              revision: 'fixture',
            }),
            { headers },
          );
        state.reads.push(args.path);
        if (args.path === 'slow.pdf')
          await new Promise<void>((resolve) => {
            state.release = resolve;
          });
        if (args.path === 'large.pdf')
          return new Response(
            JSON.stringify({ code: 'PREVIEW_TOO_LARGE', message: 'Preview supports files up to 50 MiB.' }),
            { headers: { ...headers, 'Tauri-Response': 'error' } },
          );
        return new Response(
          new Uint8Array(args.path === 'broken.pdf' ? [1, 2, 3] : args.path.endsWith('.svg') ? svg : pdf),
          { headers: { ...headers, 'Content-Type': 'application/octet-stream' } },
        );
      };
    },
    [...previewPdf()],
    [...previewSvg],
    previewMarkdown,
  );
  await $('button=Fixture previews').click();
  try {
    await $('button=Open paper.pdf').click();
    await $('.pdfViewer .textLayer').waitForExist();
    await expect($('.pdf-preview')).toHaveText(expect.stringContaining('Preview page one'));
    await expect($('button[aria-label="Save file (⌘S)"]')).not.toExist();
    await $('button[aria-label="Next page"]').click();
    await expect($('input[aria-label="Page number"]')).toHaveValue('2');
    await $('button[aria-label="Zoom in"]').click();
    await $('button=Fit width').click();
    await browser.waitUntil(() =>
      browser.execute(() => document.querySelectorAll('.pdfViewer .page[data-loaded="true"] canvas').length > 0),
    );
    await browser.saveScreenshot(new URL('../../../.artifacts/desktop-e2e/preview-pdf.png', import.meta.url).pathname);
    await $('button[aria-label="Open diagram.svg"]').click();
    await browser.waitUntil(() =>
      browser.execute(() => (document.querySelector('.image-viewport img') as HTMLImageElement)?.naturalWidth === 360),
    );
    await expect($('.image-preview')).toHaveText(expect.stringContaining('360 × 220'));
    await $('button=100%').click();
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/preview-image.png', import.meta.url).pathname,
    );
    await $('button=Open notes/readme.md').click();
    await $('.monaco-editor').waitForDisplayed();
    await $('button=Edit draft').click();
    await $('button=Split').click();
    await $('.markdown-rendered .katex').waitForDisplayed();
    await browser.waitUntil(() =>
      browser.execute(
        () => (document.querySelector('.markdown-rendered img') as HTMLImageElement)?.naturalWidth === 360,
      ),
    );
    await expect($('.markdown-rendered')).toHaveText(expect.stringContaining('Draft retained'));
    expect(
      await browser.execute(() => window.previewFixture.reads.filter((path) => path === 'notes/diagram.svg').length),
    ).toBe(1);
    await $('button=Edit draft').click();
    await expect($('.markdown-rendered')).toHaveText(expect.stringMatching(/Draft retained[\s\S]*Draft retained/));
    expect(
      await browser.execute(() => window.previewFixture.reads.filter((path) => path === 'notes/diagram.svg').length),
    ).toBe(1);
    await browser.saveScreenshot(
      new URL('../../../.artifacts/desktop-e2e/preview-markdown.png', import.meta.url).pathname,
    );
    await $('button=Preview').click();
    await expect($('.monaco-editor')).not.toExist();
    await $('button[aria-label="Save file (⌘S)"]').click();
    expect(await browser.execute(() => window.previewFixture.writes[0])).toContain('Draft retained');
    await $('.markdown-rendered').$('a=Outside').click();
    await expect($('.file-surface [role="alert"]')).toHaveText(expect.stringContaining('outside'));
    await $('.markdown-rendered').$('a=Next').click();
    await expect($('.editor-tab.selected')).toHaveText(expect.stringContaining('next.md'));
    await $('button=Open broken.pdf').click();
    await $('.pdf-preview [role="alert"]').waitForDisplayed();
    await $('button=Open large.pdf').click();
    await expect($('.editor-pane')).toHaveText(expect.stringContaining('50 MiB'));
    await expect($('.editor-pane').$('button=Download')).toBeDisplayed();
    await $('button=Open slow.pdf').click();
    await $('[aria-label="Loading file content"]').waitForDisplayed();
    await $('button[aria-label="Close slow.pdf"]').click();
    await browser.waitUntil(() => browser.execute(() => window.previewFixture.cancel > 0));
    await expect($('.editor-tab button[role="tab"][title$="/slow.pdf"]')).not.toExist();
    for (const theme of ['dark', 'light']) {
      await browser.tauri.execute(
        async ({ core }, theme) => core.invoke('app_preferences', { patch: { theme } }),
        theme,
      );
      await $('button=Open paper.pdf').click();
      await $('.pdfViewer canvas').waitForDisplayed();
    }
    await $('button[aria-label="Close paper.pdf"]').click();
    await $('button=Open paper.pdf').click();
    await $('.pdfViewer .textLayer').waitForExist();
    await expect($('.pdf-preview')).toHaveText(expect.stringContaining('Preview page one'));
    await $('button=Toggle content window').click();
    await expect($('.content-window .editor-pane')).toHaveText(expect.stringContaining('Temporary read failure'));
    await $('.content-window').$('button=Retry').click();
    await $('.content-window .monaco-editor textarea').waitForExist();
    await $('.content-window .monaco-editor textarea').addValue('Saved after retry. ');
    await expect($('.content-window .editor-status')).toHaveText(expect.stringContaining('Unsaved changes'));
    await browser.execute(() =>
      document.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 's',
          metaKey: true,
          bubbles: true,
          cancelable: true,
        }),
      ),
    );
    await browser.waitUntil(() =>
      browser.execute(() => window.previewFixture.writes.some((text) => text.includes('Saved after retry.'))),
    );
  } finally {
    await $('button=Fixture previews').click();
    await browser.execute(() => {
      window.previewFixture.release?.();
      window.previewFixture.restore();
    });
  }
}
