import { $, browser, expect } from '@wdio/globals';
import path from 'node:path';
import { createServer, type Socket } from 'node:net';
import { once } from 'node:events';
import { invokeWindow } from './invoke';
import type { Catalog } from '../src/bridge/types';
import { contextAt } from './interaction-controls';
import { sharedAppearanceAndUsage } from './settings-controls';

describe('Remote Codex desktop', () => {
  it('opens a compact native window with both sidebar trees', async () => {
    await $('section[aria-label="Connections and sessions"]').waitForDisplayed();
    await expect($('section[aria-label="Remote files"]')).toBeDisplayed();
    await expect($('button[aria-label="Refresh files"]')).toBeDisabled();
    await expect($('button[aria-label="Refresh files"] svg')).not.toHaveElementClass('spinning');
    await expect($('body')).not.toHaveText(expect.stringContaining('RESOURCE_UNAVAILABLE'));
    await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/light.png'));
    await expect($('[aria-label="Start working"]')).toHaveText(expect.stringContaining('Welcome to Remote Codex'));
    await expect($('button[aria-label="Open a file to show the editor"]')).toBeDisabled();
    await expect($('.editor-pane')).not.toExist();
    await expect($('.terminal-panel')).not.toBeDisplayed();
    await contextAt('.connections .tree-scroll', false);
    await expect($('[role="menuitem"]=Add server…')).toBeDisplayed();
    await browser.keys('Escape');
    await contextAt('.file-tree .tree-scroll', false);
    await expect($('[role="menuitem"]=New folder…')).toHaveAttribute('data-disabled');
    await browser.keys('Escape');
  });
  it('offers graphical server setup with direct connections as the default', async () => {
    await $('button[aria-label="Add server"]').click();
    await $('[role="dialog"]').waitForDisplayed();
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('SSH address'));
    await expect($('input[type="checkbox"]')).not.toBeSelected();
    await $('button[aria-label="Close"]').click();
  });
  it('persists dark appearance and supports collapsing panels', async () => {
    await $('button=Settings').click();
    await $('[role="dialog"]').waitForDisplayed();
    await $('button=Dark').click();
    await expect($('html')).toHaveAttribute('data-theme', 'dark');
    await browser.waitUntil(
      async () =>
        (await browser.tauri.execute(
          async ({ core }) => ((await core.invoke('app_preferences', { patch: null })) as { theme: string }).theme,
        )) === 'dark',
    );
    await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/settings-general-dark.png'));
    await $('[role=dialog] button[aria-label=Close]').click();
    await $('button[aria-label="Toggle terminal (⌘`)"]').click();
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('Choose a server'));
    await $('button=Cancel').click();
    const settingsBottom = await browser.execute(
      () => document.querySelector('aside > div:last-child')!.getBoundingClientRect().bottom,
    );
    await $('button=Files').click();
    expect(
      await browser.execute(() => document.querySelector('aside > div:last-child')!.getBoundingClientRect().bottom),
    ).toBe(settingsBottom);
    await $('button=Workspaces').click();
    await expect($('section[aria-label="Connections and sessions"] .section-title')).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    await $('button=Files').click();
    expect(
      await browser.execute(
        () =>
          document.querySelector('section[aria-label="Remote files"]')!.getBoundingClientRect().top >=
          document.querySelector('aside')!.getBoundingClientRect().top,
      ),
    ).toBe(true);
    await $('button=Workspaces').click();
    await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/dark.png'));
    await $('button[aria-label="Toggle sidebar (⌘B)"]').click();
    await expect($('section[aria-label="Connections and sessions"]')).not.toBeExisting();
    await $('button[aria-label="Toggle sidebar (⌘B)"]').click();
  });
  it('shares General preferences across windows and shows local account usage', sharedAppearanceAndUsage);
  it('rejects unknown resource handles through the real application API', async () => {
    const result = await browser.tauri.execute(async ({ core }) => {
      try {
        await core.invoke('terminal_input', { id: 'unowned-resource', bytes: [65] });
        return 'unexpected success';
      } catch (error) {
        return (error as { code: string }).code;
      }
    });
    expect(result).toBe('RESOURCE_UNAVAILABLE');
  });
  it('clears completed progress without clearing another pending operation', async () => {
    await $('button=Settings').waitForDisplayed();
    // A loopback peer holds the SSH handshake; no remote host or credentials are used.
    const sockets = new Set<Socket>();
    const peer = createServer((socket) => {
      sockets.add(socket);
      socket.on('error', () => {});
      socket.on('close', () => sockets.delete(socket));
    });
    const ids = [crypto.randomUUID(), crypto.randomUUID()];
    peer.listen(0, '127.0.0.1');
    await once(peer, 'listening');
    try {
      const address = peer.address();
      if (!address || typeof address === 'string') throw new Error('Loopback port unavailable');
      await browser.tauri.execute(
        ({ core }, ids, port) => {
          for (const id of ids) {
            const entry: NonNullable<Window['__remoteCodexTestRequests']>[string] = {};
            (window.__remoteCodexTestRequests ??= {})[id] = entry;
            void core
              .invoke('server_save', {
                operationId: id,
                input: {
                  name: `progress-${id}`,
                  address: 'root@127.0.0.1',
                  port,
                  identity: null,
                  password: null,
                  settings: [],
                },
              })
              .then(
                (value) => {
                  entry.result = { ok: true, value };
                },
                (error: { code: string }) => {
                  entry.result = { ok: false, error };
                },
              );
          }
        },
        ids,
        address.port,
      );
      await expect($('button[aria-label="Show 2 active operations"]')).toBeDisplayed();
      await browser.waitUntil(() => sockets.size === 2);
      await invokeWindow('main', 'cancel_operation', { id: ids[0] });
      await browser.waitUntil(() =>
        browser.execute((id) => {
          const result = window.__remoteCodexTestRequests?.[id]?.result;
          return Boolean(result && !result.ok && (result.error as { code: string }).code === 'OPERATION_CANCELLED');
        }, ids[0]),
      );
      await expect($('button[aria-label="Show 2 active operations"]')).not.toExist();
      await expect($('button[aria-label="Cancel operation"]')).toBeDisplayed();
      for (const socket of sockets) socket.destroy();
      await browser.waitUntil(() =>
        browser.execute((id) => {
          const result = window.__remoteCodexTestRequests?.[id]?.result;
          return Boolean(result && !result.ok && (result.error as { code: string }).code !== 'OPERATION_CANCELLED');
        }, ids[1]),
      );
      await expect($('button[aria-label="Cancel operation"]')).not.toExist();
      await expect($('footer [role="status"]')).not.toHaveAttribute('data-state', 'busy');
    } finally {
      for (const socket of sockets) socket.destroy();
      await new Promise<void>((resolve) => peer.close(() => resolve()));
      await Promise.all(ids.map((id) => invokeWindow('main', 'cancel_operation', { id })));
      const catalog = await invokeWindow<Catalog>('main', 'catalog', { archived: false });
      for (const server of catalog.servers.filter((s) => ids.some((id) => s.name === `progress-${id}`)))
        await invokeWindow('main', 'server_remove', { name: server.name });
      await browser.execute((ids) => {
        for (const id of ids) delete window.__remoteCodexTestRequests?.[id];
      }, ids);
    }
  });
});
