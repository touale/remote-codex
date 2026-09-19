import { $, browser, expect } from '@wdio/globals';
import { contextAt } from './interaction-controls';

declare global {
  interface Window {
    workspaceCreate: {
      requests: { command: string; args: Record<string, string> }[];
      failOpen: boolean;
      release?: () => void;
      restore: () => void;
    };
  }
}

export async function createWorkspaceFolder() {
  await browser.setWindowSize(1500, 1000);
  await browser.execute(() => {
    const original = window.fetch;
    const state: Window['workspaceCreate'] = {
      requests: [],
      failOpen: true,
      restore: () => {
        window.fetch = original;
      },
    };
    window.workspaceCreate = state;
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      const command = url.pathname.slice(1);
      if (
        url.protocol !== 'ipc:' ||
        !['directory_open', 'directory_create', 'directory_close', 'workspace_open', 'file_list'].includes(command)
      )
        return original.call(window, input, init);
      const args = JSON.parse(init!.body as string);
      state.requests.push({ command, args });
      let result: unknown = null;
      let failed = false;
      if (command === 'directory_open') result = args.server;
      if (command === 'directory_create') {
        failed = args.name === 'exists';
        if (args.name === 'new 中文')
          await new Promise<void>((resolve) => {
            state.release = resolve;
          });
        result = failed
          ? { code: 'FILE_EXISTS', message: 'Fixture folder already exists.' }
          : `${args.parent}/${args.name}`;
      }
      if (command === 'workspace_open') {
        failed = state.failOpen && args.server === 'alpha';
        result = failed
          ? { code: 'SSH_FAILED', message: 'Fixture connection interrupted.' }
          : { id: `${args.server}:${args.path}`, server: args.server, path: args.path };
      }
      if (command === 'file_list') result = { entries: [], truncated: false };
      return new Response(JSON.stringify(result), {
        headers: { 'Content-Type': 'application/json', 'Tauri-Response': failed ? 'error' : 'ok' },
      });
    };
  });
  const requests = (command: string) =>
    browser.execute(
      (command) =>
        window.workspaceCreate.requests.filter((request) => request.command === command).map((request) => request.args),
      command,
    );
  try {
    await $('button=Fixture workspace creation').click();
    await $('button=Open beta workspace').click();
    await expect($('#created-workspace')).toHaveText('beta · /workspace/a');
    const server = '.tree-node:has(> [data-context-menu] > .server-row[data-server-id="alpha"])';
    const group = `${server} .directory-row[data-directory-path="/workspace"]`;
    const leaf = `${server} .workspace-row[data-workspace-path="/workspace/a"]`;
    await $(group).waitForDisplayed();
    await contextAt(group);
    await $('[role="menuitem"]=New folder…').click();
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('New folder · alpha'));
    await $('button=Cancel new folder').click();
    expect(await requests('directory_open')).toEqual([]);
    await contextAt(group);
    await $('[role="menuitem"]=New folder…').click();
    const input = $('.new-workspace-folder input');
    await input.setValue('../escape');
    await $('button=Create folder').click();
    await expect($('[role="dialog"] [role="alert"]')).toHaveText(expect.stringContaining('single folder name'));
    expect(await requests('directory_open')).toEqual([]);
    await input.setValue('exists');
    await $('button=Create folder').click();
    await expect($('[role="dialog"] [role="alert"]')).toHaveText('Fixture folder already exists.');
    await expect(input).toHaveValue('exists');
    await input.setValue('new 中文');
    await browser.execute(() => {
      const form = document.querySelector('.new-workspace-folder')!;
      for (let i = 0; i < 2; i++) form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    });
    await browser.waitUntil(() => browser.execute(() => Boolean(window.workspaceCreate.release)));
    await expect($('button=Creating…')).toBeDisabled();
    await browser.keys('Escape');
    await expect($('[role="dialog"]')).toBeDisplayed();
    expect(
      (await requests('directory_create')).map(({ browser, parent, name }) => ({ browser, parent, name })),
    ).toEqual([
      { browser: 'alpha', parent: '/workspace', name: 'exists' },
      { browser: 'alpha', parent: '/workspace', name: 'new 中文' },
    ]);
    expect(await requests('directory_open')).toHaveLength(1);
    await browser.execute(() => window.workspaceCreate.release!());
    await expect($('button=Retry opening')).toBeEnabled();
    await expect($('[role="dialog"]')).toHaveText(
      expect.stringContaining('Folder created, but the workspace could not be opened.'),
    );
    await browser.execute(() => {
      window.workspaceCreate.failOpen = false;
    });
    await $('button=Retry opening').click();
    await expect($('[role="dialog"]')).not.toExist();
    await expect($('#created-workspace')).toHaveText('alpha · /workspace/new 中文');
    await expect($('.workspace-path')).toHaveText('/workspace/new 中文');
    await expect($(`${server} .workspace-row[data-workspace-path="/workspace/new 中文"]`)).toBeDisplayed();
    expect(await requests('directory_create')).toHaveLength(2);
    await browser.waitUntil(async () => (await requests('directory_close')).length === 1);
    await browser.execute((selector) => {
      document
        .querySelector(`${selector} .menu-trigger`)!
        .dispatchEvent(
          new PointerEvent('pointerdown', { bubbles: true, cancelable: true, pointerType: 'mouse', button: 0 }),
        );
    }, leaf);
    await $('[role="menu"]').waitForDisplayed();
    await $('[role="menuitem"]=New folder…').click();
    await input.setValue('child');
    await $('button=Create folder').click();
    await expect($('[role="dialog"]')).not.toExist();
    await expect($('#created-workspace')).toHaveText('alpha · /workspace/a/child');
    await expect($(`${server} .workspace-row[data-workspace-path="/workspace/a/child"]`)).toBeDisplayed();
    expect((await requests('directory_create')).at(-1)).toMatchObject({
      browser: 'alpha',
      parent: '/workspace/a',
      name: 'child',
    });
    await browser.waitUntil(async () => (await requests('directory_close')).length === 2);
  } finally {
    await browser.execute(() => window.workspaceCreate.release?.());
    await $('button=Fixture workspace creation').click();
    await browser.execute(() => window.workspaceCreate.restore());
  }
}
