import { $, browser, expect } from '@wdio/globals';
import { withExecuteOptions } from '@wdio/tauri-service';
import type { Catalog, SessionOpened, WindowTarget } from '../src/bridge/types';
import { contextAt } from './interaction-controls';
import { invokeWindow } from './invoke';

export async function windowNavigation(remote: string, session: string) {
  const main = await browser.getWindowHandle();
  const catalog = await invokeWindow<Catalog>('main', 'catalog', { archived: false });
  const server = catalog.servers.find((s) => s.name === 'desktop-test')!;
  const cases: [string, WindowTarget][] = [
    [`.server-row[data-server-id="${server.id}"] .tree-label`, { kind: 'server', server: server.name }],
    [`[data-workspace-path="${remote}"] .tree-label`, { kind: 'workspace', server: server.name, path: remote }],
    [`[data-session-id="${session}"] .tree-label`, { kind: 'session', id: session }],
  ];
  for (const [selector, target] of cases) {
    await contextAt(selector);
    await $('[role="menuitem"]=Open in New Window').click();
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 2);
    const other = (await browser.getWindowHandles()).find((h) => h !== main)!;
    await browser.switchToWindow(other);
    const label = other;
    try {
      await expect($('.workspace-path')).toHaveText(target.kind === 'server' ? '/' : remote);
      if (target.kind === 'server') {
        await expect($('[aria-label="Server home"]')).toHaveText(expect.stringContaining(server.name));
        await $('.file-row .tree-label[title="tmp"]').waitForDisplayed();
      } else if (target.kind === 'workspace') {
        await expect($('[aria-label="Start working"]')).toHaveText(expect.stringContaining(remote));
        await cancelledTrust(remote, session);
      } else {
        await $('textarea[aria-label="Message Codex"]').waitForDisplayed({ timeout: 60000 });
        await expect($('footer')).toHaveAttribute('data-session-id', session);
        await contextAt(`[data-session-id="${session}"] .tree-label`);
        await expect($('[role="menuitem"]=Open in New Window')).toHaveAttribute('data-disabled');
        await browser.keys('Escape');
        await expect(invokeWindow('main', 'new_window', { target })).rejects.toMatchObject({ code: 'SESSION_IN_USE' });
        expect(await browser.getWindowHandles()).toHaveLength(2);
        await browser.switchToWindow(main);
        await contextAt(selector);
        await expect($('[role="menuitem"]=Open in New Window')).toHaveAttribute('data-disabled');
        await browser.keys('Escape');
      }
    } finally {
      await browser.tauri.execute(
        ({ core }) => {
          setTimeout(() => {
            void core.invoke('close_window', { cancel: false });
          }, 50);
          return true;
        },
        withExecuteOptions({ windowLabel: label }),
      );
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 1);
      await browser.switchToWindow(main);
    }
  }
}

async function cancelledTrust(remote: string, session: string) {
  const workspace = await invokeWindow<{ id: string }>('main', 'workspace_open', {
    operationId: crypto.randomUUID(),
    server: 'desktop-test',
    path: remote,
  });
  const context = workspace.id;
  await invokeWindow('main', 'file_change', { context, change: { action: 'directory', path: '.codex' } });
  try {
    await invokeWindow('main', 'file_write', {
      context,
      path: '.codex/config.toml',
      revision: null,
      text: '[mcp_servers.cancel_probe]\ncommand="sh"\nargs=["-c", "sleep 60"]\nstartup_timeout_sec=60\nrequired=true\n',
    });
    const prepared = await invokeWindow<SessionOpened>('main', 'session_open', {
      operationId: crypto.randomUUID(),
      input: { server: 'desktop-test', path: remote, resume: session, takeover: false, mcp_source: null },
    });
    if (prepared.status !== 'trust') throw new Error('The project MCP must require trust.');
    const selector = `[data-session-id="${session}"] .tree-label`;
    await contextAt(selector);
    await expect($('[role="menuitem"]=Open in New Window')).toHaveAttribute('data-disabled');
    await browser.keys('Escape');
    const operationId = crypto.randomUUID();
    await browser.tauri.execute(
      ({ core }, preparation, operationId) => {
        const state = globalThis as typeof globalThis & { trustCancellation?: string };
        state.trustCancellation = undefined;
        void core.invoke('session_trust', { preparation, operationId, accept: true }).then(
          () => {
            state.trustCancellation = 'opened';
          },
          (error: { code: string }) => {
            state.trustCancellation = error.code;
          },
        );
      },
      withExecuteOptions({ windowLabel: 'main' }),
      prepared.preparation,
      operationId,
    );
    await browser.waitUntil(
      async () =>
        (await browser.tauri.execute(
          async ({ core }, operationId) => {
            await core.invoke('cancel_operation', { id: operationId });
            return (globalThis as typeof globalThis & { trustCancellation?: string }).trustCancellation;
          },
          withExecuteOptions({ windowLabel: 'main' }),
          operationId,
        )) === 'OPERATION_CANCELLED',
    );
    // The other window must update from the event, without a manual catalog refresh.
    await browser.waitUntil(async () => {
      await contextAt(selector);
      const disabled = await $('[role="menuitem"]=Open in New Window').getAttribute('data-disabled');
      await browser.keys('Escape');
      return disabled === null;
    });
  } finally {
    await invokeWindow('main', 'file_change', { context, change: { action: 'remove', path: '.codex/config.toml' } });
  }
}
