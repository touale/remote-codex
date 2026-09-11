import { $, browser, expect } from '@wdio/globals';
import path from 'node:path';
import { activateTree, rowMenu } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

async function alignedEditor() {
  const geometry = await browser.execute(() => {
    const heading = document.querySelector('[aria-label="Conversation heading"]');
    const chat = (heading ?? document.querySelector('[aria-label="Start working"]'))!.getBoundingClientRect();
    const editor = document.querySelector('.editor-header')!.getBoundingClientRect();
    const divider = document.querySelector('[aria-label="Resize editor"]')!.getBoundingClientRect();
    return {
      chatTop: chat.top,
      editorTop: editor.top,
      chatBottom: heading ? chat.bottom : null,
      editorBottom: editor.bottom,
      height: editor.height,
      dividerTop: divider.top,
    };
  });
  expect(geometry.editorTop).toBe(geometry.chatTop);
  if (geometry.chatBottom !== null)
    expect(Math.abs(geometry.editorBottom - geometry.chatBottom)).toBeLessThanOrEqual(1);
  expect(geometry.height).toBeGreaterThan(0);
  expect(geometry.dividerTop).toBe(geometry.editorTop);
  expect(
    await browser.execute(() => document.querySelectorAll('.editor-pane .pane-toolbar, .file-breadcrumb').length),
  ).toBe(0);
}

export async function workspaceGroups(invoke: Invoke, workspace: string, remote: string) {
  for (const name of ['projects', 'projects/test', 'projects/test1'])
    await invoke('file_change', { context: workspace, change: { action: 'directory', path: name } });
  for (const name of ['test', 'test1'])
    await invoke('workspace_open', {
      operationId: crypto.randomUUID(),
      server: 'desktop-test',
      path: `${remote}/projects/${name}`,
    });
  const group = `[data-directory-path="${remote}/projects"] .tree-label`;
  const child = (name: string) => `[data-workspace-path="${remote}/projects/${name}"] .tree-label`;
  await $(child('test1')).waitForDisplayed();
  await expect($(group).$('svg.lucide-folder-open')).toExist();
  await expect($(child('test'))).toHaveText(expect.stringContaining('test'));
  await rowMenu(`[data-directory-path="${remote}/projects"]`, 'Copy path');
  await $(group).click();
  await expect($(group).$('svg.lucide-folder')).toExist();
  await expect($(child('test1'))).not.toExist();
  await expect($('.workspace-path')).toHaveText(remote);
  const search = $('input[aria-label="Search workspaces and sessions"]');
  await search.setValue('test1');
  await $(child('test1')).waitForDisplayed();
  await expect($(group).$('svg.lucide-folder-open')).toExist();
  await expect($(child('test'))).not.toExist();
  await expect($(group)).toHaveText('projects');
  await activateTree(child('test1'));
  await expect($('.workspace-path')).toHaveText(`${remote}/projects/test1`);
  await search.clearValue();
  await expect($(child('test'))).toBeDisplayed();
  await activateTree(`[data-workspace-path="${remote}"] .tree-label`);
  await expect($('.workspace-path')).toHaveText(remote);
  await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/workspace-groups.png'));
}

export async function editorTabs(remote: string) {
  for (const name of [
    'a-very-long-example-file-name-for-editor-tabs.md',
    'another-long-file-name-for-editor-tabs.md',
  ]) {
    await $('button[aria-label="New file"]').click();
    await $('[role="dialog"] input').setValue(name);
    await $('button=Create').click();
    await $(`[role="tab"][title="desktop-test · ${remote}/${name}"]`).waitForDisplayed();
  }
  await expect($('button[aria-label="Hide editor"]')).toBeDisplayed();
  await $('[role="separator"][aria-label="Resize editor"]').click();
  await browser.keys('ArrowLeft');
  await alignedEditor();
  // Select an earlier off-screen tab by scrolling the tab strip, not the window.
  await browser.execute(() => {
    document.querySelector('.editor-tabs')!.scrollLeft = 0;
  });
  await $(`[role="tab"][title="desktop-test · ${remote}/notes.md"]`).click();
  await expect($('button[aria-label="Save file (⌘S)"]')).toBeDisplayed();
}
