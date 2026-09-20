import { $, browser, expect } from '@wdio/globals';

declare global {
  interface Window {
    linkFixture: {
      reads: { context: string; path: string }[];
      external: string[];
      release?: () => void;
      restore: () => void;
    };
  }
}

export async function fileLinks() {
  await browser.setWindowSize(1440, 1000);
  await browser.execute(() => {
    const original = window.fetch;
    const state = (window.linkFixture = {
      reads: [],
      external: [],
      restore: () => {
        window.fetch = original;
      },
    } as Window['linkFixture']);
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      if (url.protocol !== 'ipc:' || !['/workspace_open', '/file_read', '/external_link'].includes(url.pathname))
        return original.call(window, input, init);
      const args = JSON.parse(init!.body as string);
      let result: unknown = null;
      let failed = false;
      if (url.pathname === '/workspace_open') result = { id: args.server, server: args.server, path: args.path };
      if (url.pathname === '/external_link') state.external.push(args.url);
      if (url.pathname === '/file_read') {
        state.reads.push(args);
        if (args.path === 'delayed.md')
          await new Promise<void>((resolve) => {
            state.release = resolve;
          });
        failed = args.path === 'missing.md';
        result = failed
          ? { code: 'FILE_NOT_FOUND', message: 'Fixture file not found.' }
          : { path: args.path, text: `${args.context}: ${args.path}`, revision: 'fixture' };
      }
      return new Response(JSON.stringify(result), {
        headers: { 'Content-Type': 'application/json', 'Tauri-Response': failed ? 'error' : 'ok' },
      });
    };
  });
  await $('button=Fixture links').click();
  try {
    await $('button=Open alpha').click();
    await $('a=Absolute').waitForDisplayed();
    await $('a=Absolute').click();
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/alpha:\sdecision\.md/));
    // A second spelling of the same file must preserve unsaved editor content.
    await $('.monaco-editor textarea').addValue('draft');
    await $('a=Relative').click();
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringContaining('draft'));
    expect(await browser.execute(() => window.linkFixture.reads.length)).toBe(1);
    await $('a=Encoded').click();
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/中文\sfile\.md/));
    await $('a=Outside').click();
    await expect($('#link-error')).toHaveText('This file is outside the current conversation workspace.');
    expect(await browser.execute(() => window.linkFixture.reads.length)).toBe(2);
    await $('a=Website').click();
    expect(await browser.execute(() => window.linkFixture.external)).toEqual(['https://example.com/reference']);
    await $('a=Missing').click();
    await expect($('.editor-pane')).toHaveText(expect.stringContaining('Fixture file not found.'));
    await expect($('.editor-pane').$('button=Retry')).toBeDisplayed();
    await $('a=Delayed').click();
    await browser.waitUntil(() => browser.execute(() => Boolean(window.linkFixture.release)));
    await $('button=Open beta').click();
    await expect($('#link-session')).toHaveText('beta');
    await $('a=Absolute').click();
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/beta:\sdecision\.md/));
    await browser.execute(() => window.linkFixture.release!());
    await $('button=Open alpha').click();
    await expect($('#link-session')).toHaveText('alpha');
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/alpha:\sdelayed\.md/));
    await $('button=Open beta').click();
    await expect($('#link-session')).toHaveText('beta');
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringMatching(/beta:\sdecision\.md/));
    await $('button=Open alpha').click();
    await expect($('#link-session')).toHaveText('alpha');
    await $('a=Relative').click();
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringContaining('draft'));
    await browser.setWindowSize(1000, 900);
    await $('a=Summary').click();
    await expect($('.editor-pane')).toBeDisplayed();
    await expect($('.monaco-editor .view-lines')).toHaveText(expect.stringContaining('program/5/result_A4/summary.md'));
    for (const width of [900, 1440]) {
      await browser.setWindowSize(width, 900);
      await expect($('.editor-pane')).toBeDisplayed();
      await expect($('section[aria-label="Conversation"]')).toBeDisplayed();
      await expect($('[role="separator"][aria-label="Resize editor"]')).toBeDisplayed();
    }
    // A wide window can still have little room after expanding its sidebar.
    await browser.execute(() => {
      const content = document.querySelector('.editor-pane')!.parentElement!.parentElement!;
      content.style.width = '600px';
    });
    await browser.waitUntil(() =>
      browser.execute(() => {
        const editor = document.querySelector('.editor-pane')!;
        const content = editor.parentElement!.parentElement!;
        const bounds = content.getBoundingClientRect();
        const file = editor.getBoundingClientRect();
        const chat = content.querySelector('.chat')!.getBoundingClientRect();
        return chat.width > 0 && file.width > 0 && chat.left >= bounds.left && file.right <= bounds.right + 1;
      }),
    );
  } finally {
    await browser.execute(() => {
      window.linkFixture.release?.();
      window.linkFixture.restore();
    });
    await $('button=Fixture links').click();
  }
}
