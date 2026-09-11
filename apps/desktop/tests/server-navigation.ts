import { $, browser, expect } from '@wdio/globals';
import type { Catalog, Server, TextFile } from '../src/bridge/types';
import { masters } from './directory-lifecycle';
import { activateTree, contextAt } from './interaction-controls';
import { emptyServerFiles } from './server-files';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function serverNavigation(invoke: Invoke, workspace: string, remote: string, session: string) {
  const savedFile = await invoke<TextFile>('file_read', { context: workspace, path: 'notes.md' });
  await $('.monaco-editor textarea').addValue(' Retained across servers.');
  await $('button[aria-label="Save file (⌘S)"]').waitForEnabled();
  const secondary = await invoke<Server>('server_save', {
    operationId: crypto.randomUUID(),
    input: {
      name: 'desktop-secondary',
      address: process.env.REMOTE_CODEX_E2E_TARGET!,
      port: Number(process.env.REMOTE_CODEX_E2E_PORT),
      identity: process.env.REMOTE_CODEX_E2E_IDENTITY || null,
      password: null,
      settings: [],
    },
  });
  const first = (await invoke<Catalog>('catalog', { archived: false })).servers.find((s) => s.name === 'desktop-test')!;
  const row = (id: string) => `.server-row[data-server-id="${id}"]`;
  const home = '[aria-label="Server home"]';
  await $(row(secondary.id)).waitForDisplayed();
  await $('.xterm-helper-textarea').addValue("sleep 1; printf 'background-ok\\n' > server-switch.txt\r");
  await contextAt(`[data-workspace-path="${remote}"] .tree-label`);
  await $('[role="menuitem"]=New session').click();
  await $('textarea[aria-label="Message Codex"]').setValue('Keep this draft on the first server.');
  const connections = masters().length;
  await $(row(secondary.id)).$('.tree-label').click();
  await expect($('textarea[aria-label="Message Codex"]')).toHaveValue('Keep this draft on the first server.');
  expect(masters().length).toBe(connections);
  await emptyServerFiles(invoke, secondary, first, remote);
  const fileConnections = masters().length;
  await expect($(home)).toHaveText(expect.stringContaining('desktop-secondary'));
  await expect($(home)).not.toHaveText(expect.stringContaining('desktop-test'));
  await expect($(home).$('button[aria-label="Add workspace"]')).toBeDisplayed();
  await expect($('[aria-label="Recent sessions"]')).not.toExist();
  await expect($('.editor-pane')).not.toExist();
  await expect($('.terminal-panel')).not.toBeDisplayed();
  await expect($('.workspace-path')).not.toHaveText(remote);
  await expect($('textarea[aria-label="Message Codex"]')).not.toExist();
  await $(row(secondary.id)).$('.tree-disclosure').click();
  await expect($(row(secondary.id)).$('.lucide-folder')).toExist();
  await expect($(home)).toHaveAttribute('data-server-id', secondary.id);
  await activateTree(`${row(secondary.id)} .tree-label`);
  await expect($(row(secondary.id)).$('.lucide-folder-open')).toExist();
  expect(masters().length).toBe(fileConnections);
  await $(home).$('button[aria-label="Add workspace"]').click();
  await expect($('.workspace-dialog')).toHaveText(expect.stringContaining('Add workspace · desktop-secondary'));
  await $('.workspace-dialog button[aria-label="Close"]').click();
  await invoke('workspace_open', {
    operationId: crypto.randomUUID(),
    server: secondary.name,
    path: `${remote}/cold-b`,
  });
  await expect($('[aria-label="Server workspaces"] button')).toHaveText(`${remote}/cold-b`);
  await activateTree(`${row(first.id)} .tree-label`);
  await expect($('.file-tree')).toHaveAttribute('data-server', first.name);
  await expect($('.workspace-path')).toHaveText('/');
  await $('.file-row .tree-label[title="tmp"]').waitForDisplayed();
  await activateTree(`${row(secondary.id)} .tree-label`);
  await expect($('.workspace-path')).toHaveText('/');
  await expect($('.file-tree')).toHaveAttribute('data-server', secondary.name);
  await $('.file-row .tree-label[title="tmp"]').waitForDisplayed();
  expect(
    (await invoke<Catalog>('catalog', { archived: false })).workspaces.filter((w) => w.server_id === secondary.id),
  ).toHaveLength(1);
  const beforePicker = masters().length;
  await $(home).$('button[aria-label="New session"]').click();
  await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('Choose a workspace · desktop-secondary'));
  await expect($('[role="dialog"]')).not.toHaveText(expect.stringContaining('desktop-test'));
  await $('button=Cancel').click();
  expect(masters().length).toBe(beforePicker);
  await $('button[aria-label="Toggle terminal (⌘`)"]').click();
  await $('.terminal-panel').waitForDisplayed();
  await expect($('[role="dialog"]')).not.toExist();
  expect(await $('.terminal-tab.selected > button').getText()).toMatch(/^\//);
  await expect($('.terminal-tab.selected')).not.toHaveText(expect.stringContaining(remote));
  await $('button[aria-label="Close terminal"]').click();
  await browser.saveScreenshot(
    new URL('../../../.artifacts/desktop-e2e/server-home-light.png', import.meta.url).pathname,
  );
  await activateTree(`${row(first.id)} .tree-label`);
  await expect($('[aria-label="Recent sessions"]')).toHaveText(expect.stringContaining('desktop-test'));
  await $(home).$('button[aria-label="New session"]').click();
  await $('[role="dialog"]').$(`span=desktop-test · ${remote}`).click();
  await expect($('textarea[aria-label="Message Codex"]')).toHaveValue('Keep this draft on the first server.');
  await $('textarea[aria-label="Message Codex"]').setValue('');
  await expect($('button[aria-label="Save file (⌘S)"]')).toBeEnabled();
  await expect($('.monaco-editor')).toBeDisplayed();
  // Monaco virtualizes rendered lines; verify retained edits through the actual save below.
  expect((await invoke<TextFile>('file_read', { context: workspace, path: 'notes.md' })).revision).toBe(
    savedFile.revision,
  );
  await $('button[aria-label="Save file (⌘S)"]').click();
  await browser.waitUntil(async () =>
    (await invoke<TextFile>('file_read', { context: workspace, path: 'notes.md' })).text.includes(
      'Retained across servers.',
    ),
  );
  await browser.waitUntil(
    async () =>
      (await invoke<TextFile>('file_read', { context: workspace, path: 'server-switch.txt' })).text ===
      'background-ok\n',
  );
  expect((await invoke<Catalog>('catalog', { archived: false })).live.some((c) => c.session.id === session)).toBe(true);
  await activateTree(`${row(secondary.id)} .tree-label`);
  await invoke('server_remove', { name: secondary.name });
  await expect($('[aria-label="Start working"]')).toHaveText(expect.stringContaining('Welcome to Remote Codex'));
  await $(`[data-session-id="${session}"] .tree-label`).click();
}
