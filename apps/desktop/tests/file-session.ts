import { $, browser, expect } from '@wdio/globals';
import { activateTree, contextAt } from './interaction-controls';

declare global {
  interface Window {
    fileSession: { requests: { command: string; args: Record<string, string> }[]; fail: string; restore: () => void };
  }
}

export async function fileFolderSessions() {
  await browser.setWindowSize(1200, 900);
  await browser.execute(() => {
    for (const server of ['alpha', 'beta'])
      for (const path of ['/', '/workspace/a', '/workspace/a/src/项目 space', '/src'])
        localStorage.removeItem(`file-tree:${JSON.stringify([server, path])}`);
    const original = window.fetch;
    const theme = document.documentElement.dataset.theme;
    const state = (window.fileSession = {
      requests: [],
      fail: '',
      restore: () => {
        window.fetch = original;
        if (theme) document.documentElement.dataset.theme = theme;
      },
    });
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      const command = url.pathname.slice(1);
      if (
        url.protocol !== 'ipc:' ||
        !['workspace_open', 'file_context_open', 'file_context_close', 'file_list', 'session_open'].includes(command)
      )
        return original.call(window, input, init);
      const args = JSON.parse(init!.body as string);
      window.fileSession.requests.push({ command, args });
      const failed = command === 'workspace_open' && args.path === state.fail;
      let result: unknown = null;
      if (command === 'workspace_open' || command === 'file_context_open')
        result = failed
          ? { code: 'SSH_FAILED', message: 'Fixture connection failed.' }
          : { id: `${args.server}:${args.path}`, server: args.server, path: args.path, kind: 'server' };
      if (command === 'file_list') {
        const names =
          args.context.endsWith('/src/项目 space') || args.context.endsWith(':/src')
            ? []
            : args.path === ''
              ? ['src', 'empty', 'readme.md']
              : args.path === 'src'
                ? ['empty', '项目 space']
                : [];
        result = {
          entries: names.map((name: string) => ({
            name,
            path: args.path ? `${args.path}/${name}` : name,
            directory: !name.endsWith('.md'),
            symlink: false,
            size: 0,
          })),
          truncated: false,
        };
      }
      return new Response(JSON.stringify(result), {
        headers: { 'Content-Type': 'application/json', 'Tauri-Response': failed ? 'error' : 'ok' },
      });
    };
  });
  try {
    await $('button=Fixture workspace creation').click();
    await $('button=Open beta workspace').click();
    const row = (path: string) => `.file-row[data-path=${JSON.stringify(path)}]`;
    await $(row('src')).waitForDisplayed();
    await contextAt(row('readme.md'));
    await expect($('[role="menuitem"]=New session')).not.toExist();
    await browser.keys('Escape');
    for (const path of ['empty', 'src', 'src/empty']) {
      await $(row(path)).waitForDisplayed();
      await $(`${row(path)} .tree-label`).click();
    }
    await browser.waitUntil(() =>
      browser.execute(() => document.querySelectorAll('.file-tree .tree-hint').length === 2),
    );
    await browser.execute(() => {
      document.querySelector<HTMLElement>('aside')!.style.width = '180px';
    });
    const offsets = await browser.execute(() =>
      [...document.querySelectorAll('.file-tree .tree-hint')].map((hint) => {
        const label = hint.closest('.tree-node')!.querySelector('.tree-label > span')!;
        const text = document.createRange();
        text.selectNodeContents(hint);
        return Math.abs(text.getBoundingClientRect().left - label.getBoundingClientRect().left);
      }),
    );
    expect(offsets.every((offset) => offset < 1)).toBe(true);
    for (const theme of ['light', 'dark']) {
      await browser.execute((theme) => {
        document.documentElement.dataset.theme = theme;
      }, theme);
      await browser.saveScreenshot(
        new URL(`../../../.artifacts/desktop-e2e/file-folder-${theme}.png`, import.meta.url).pathname,
      );
    }
    const target = '/workspace/a/src/项目 space';
    const open = async () => {
      await $('button=Open beta workspace').click();
      await $(row('src/项目 space')).waitForDisplayed();
      await contextAt(row('src/项目 space'));
      await $('[role="menuitem"]=New session').click();
      await expect($('#created-workspace')).toHaveText(`beta · ${target}`);
      await expect($('.workspace-path')).toHaveText(target);
      await $(`[data-workspace-path=${JSON.stringify(target)}]`).waitForDisplayed();
    };
    await open();
    const draft = await $('#workspace-draft').getText();
    expect(draft.length).toBeGreaterThan(0);
    await open();
    await expect($('#workspace-draft')).toHaveText(draft);
    expect(
      await browser.execute(
        (path) => document.querySelectorAll(`[data-workspace-path=${JSON.stringify(path)}]`).length,
        target,
      ),
    ).toBe(1);

    await activateTree('.server-row[data-server-id="alpha"] .tree-label');
    await $(row('src')).waitForDisplayed();
    await browser.execute(() => {
      window.fileSession.fail = '/src';
    });
    await contextAt(row('src'));
    await $('[role="menuitem"]=New session').click();
    await expect($('#created-workspace')).toHaveText('alpha · /src');
    await expect($('.file-tree [role="alert"]')).toHaveText('Fixture connection failed.');
    await browser.execute(() => {
      window.fileSession.fail = '';
    });
    await $('.file-tree').$('button=Retry').click();
    await $('[data-workspace-path="/src"]').waitForDisplayed();
    const requests = await browser.execute(() => window.fileSession.requests);
    expect(
      requests.filter((r) => r.command === 'workspace_open' && r.args.server === 'beta' && r.args.path === target)
        .length,
    ).toBe(1);
    expect(requests.filter((r) => r.command === 'session_open').length).toBe(0);
  } finally {
    await $('button=Fixture workspace creation').click();
    await browser.execute(() => window.fileSession.restore());
  }
}
