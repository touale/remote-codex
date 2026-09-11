import { $, browser, expect } from '@wdio/globals';
import { withExecuteOptions } from '@wdio/tauri-service';
import path from 'node:path';
import type { Catalog, SessionSnapshot, TextFile } from '../src/bridge/types';
import { archiveAndRestore } from './archive-controls';
import { conversationLoading } from './conversation-loading';
import { draftModes, immediateDraft, usagePreferences } from './draft-controls';
import { fileCreation } from './experience-controls';
import { directoryLoading, editorLoading } from './file-loading';
import { fileTransfers } from './file-transfers';
import { firstMessage } from './first-message';
import { activateTree, composerInput } from './interaction-controls';
import { invokeWindow } from './invoke';
import { editorTabs, workspaceGroups } from './layout-controls';
import { refreshTree } from './navigation-controls';
import { selectOption } from './scroll-controls';
import { serverNavigation } from './server-navigation';
import { sessionControls } from './session-controls';
import { startupWithoutConnection } from './settings-controls';
import { workspaceFolderCreation } from './workspace-folder';
import { initializeWorkspace } from './workspace-initialize';

describe('A real SSH workspace in the native desktop', () => {
  it('completes the workspace and session lifecycle', async () => {
    await browser.setTimeout({ script: 120000 });
    const remote = process.env.REMOTE_CODEX_E2E_WORKSPACE!;
    let windowLabel = 'main';
    const invoke = <T>(command: string, args: Record<string, unknown> = {}) =>
      invokeWindow<T>(windowLabel, command, args);
    let workspace: string;
    let session: string;
    let savedUsage: SessionSnapshot['status']['usage'];
    const step = async (name: string, action: () => Promise<void>) => {
      console.info(`Workspace lifecycle: ${name}`);
      try {
        await action();
      } catch (error) {
        if (error instanceof Error) {
          error.message = `Workspace lifecycle failed at ${name}: ${error.message}`;
          throw error;
        }
        throw new Error(`Workspace lifecycle failed at ${name}: ${String(error)}`);
      }
    };
    await step('initializes an execution host and opens its file tree', async () => {
      workspace = await initializeWorkspace(invoke, remote);
    });
    await step('creates folders while adding a workspace', async () => {
      await workspaceFolderCreation(invoke, remote);
    });
    await step('loads directories with scoped skeletons and rejects stale refreshes', async () => {
      await directoryLoading(invoke, workspace, remote);
    });
    await step('opens file tabs immediately and preserves selection across delayed reads', async () => {
      await editorLoading(invoke, remote);
    });
    await step('creates and edits a remote file with a checked revision', async () => {
      await fileCreation(invoke, workspace, remote);
      await $('button[aria-label="New file"]').click();
      await $('[role="dialog"] input').setValue('notes.md');
      await $('button=Create').click();
      await $('.monaco-editor textarea').waitForExist({ timeout: 30000 });
      await $('.monaco-editor textarea').addValue('Desktop editing works.');
      await $('button[aria-label="Save file (⌘S)"]').waitForEnabled();
      await $('button[aria-label="Save file (⌘S)"]').click();
      await browser.waitUntil(async () =>
        (await invoke<TextFile>('file_read', { context: workspace, path: 'notes.md' })).text.includes(
          'Desktop editing works.',
        ),
      );
      const original = await invoke<TextFile>('file_read', { context: workspace, path: 'notes.md' });
      await invoke('file_write', {
        context: workspace,
        path: 'notes.md',
        text: 'Changed outside the editor.',
        revision: original.revision,
      });
      await $('.monaco-editor textarea').addValue(' More edits.');
      await $('button[aria-label="Save file (⌘S)"]').click();
      await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('This file changed remotely'));
      await $('button=Discard my edits').click();
      await expect($('.editor-status')).toHaveText(expect.stringContaining('Saved'));
      await editorTabs(remote);
    });
    await step('preserves dirty buffers across workspaces and cancelling tab closure', async () => {
      await $('.monaco-editor textarea').addValue(' Kept across workspaces.');
      await invoke('file_change', { context: workspace, change: { action: 'directory', path: 'second' } });
      await invoke('workspace_open', {
        operationId: crypto.randomUUID(),
        server: 'desktop-test',
        path: `${remote}/second`,
      });
      await activateTree(`button[title="${remote}/second"]`);
      await expect($('.workspace-path')).toHaveText(`${remote}/second`);
      await activateTree(`button[title="${remote}"]`);
      await expect($('.dirty-dot')).toBeDisplayed();
      await $('button[aria-label="Close notes.md"]').click();
      await $('button=Cancel').click();
      await expect($('.dirty-dot')).toBeDisplayed();
      await $('button[aria-label="Save file (⌘S)"]').click();
      await browser.waitUntil(async () =>
        (await invoke<TextFile>('file_read', { context: workspace, path: 'notes.md' })).text.includes(
          'Kept across workspaces.',
        ),
      );
      await $('button=Settings').click();
      await $('button=MCP').click();
      await expect($('[aria-label="MCP workspace"]')).toHaveText(`desktop-test · ${remote}`);
      await $('.settings-fields:not([disabled])').waitForExist();
      await $('.mcp-list button:last-child').click();
      await $('.mcp-definition input').waitForDisplayed();
      await $('.mcp-definition input').setValue('unsaved-server');
      await selectOption('[aria-label="MCP workspace"]', `desktop-test · ${remote}/second`);
      await $('button=Cancel').click();
      await expect($('.mcp-definition input')).toHaveValue('unsaved-server');
      await $('button=General').click();
      await $('button=Discard changes').click();
      await $('[role=dialog] button[aria-label=Close]').click();
      await expect($('.workspace-path')).toHaveText(remote);
      await workspaceGroups(invoke, workspace, remote);
      await fileTransfers(invoke, workspace, remote);
    });
    await step('streams a local Codex conversation and acknowledges native settings', async () => {
      await immediateDraft(invoke);
      await $('textarea[aria-label="Message Codex"]').waitForDisplayed({ timeout: 60000 });
      await $('textarea[aria-label="Message Codex"]').setValue('Check this remote workspace.');
      await composerInput();
      await expect($('textarea[aria-label="Message Codex"]')).toHaveValue('Check this remote workspace.');
      await firstMessage();
      await expect($('.messages')).toHaveText(expect.stringContaining('The remote workspace is ready.'));
      await $('.working').waitForExist({ reverse: true });
      const catalog = await invoke<Catalog>('catalog', { archived: false });
      session = catalog.live[0].session.id;
      await $('button[aria-label="Permissions"]').click();
      await $('.permissions-menu').$('button*=Full Access').click();
      await $('button=Cancel').click();
      expect(
        (await invoke<{ settings: { full_access: boolean } }>('session_snapshot', { id: session })).settings
          .full_access,
      ).toBe(false);
      await $('button[aria-label="Permissions"]').click();
      await $('.permissions-menu').$('button*=Full Access').click();
      await $('button=Enable Full Access').click();
      await expect($('button[aria-label="Permissions"]')).toHaveText(expect.stringContaining('Full Access'));
      await $('button[aria-label="Permissions"]').click();
      await $('.permissions-menu').$('button*=Ask for approval').click();
      await browser.waitUntil(
        async () =>
          (await invoke<{ settings: { reviewer: string } }>('session_snapshot', { id: session })).settings.reviewer ===
          'user',
      );
      await $('button[aria-label="Model and reasoning"]').click();
      await $('button=Other model…').click();
      await $('input[aria-label="Custom model ID"]').setValue('gpt-5.4-mini');
      await $('button=Use model').click();
      await browser.waitUntil(
        async () =>
          (await invoke<{ settings: { model: string } }>('session_snapshot', { id: session })).settings.model ===
          'gpt-5.4-mini',
      );
    });
    await step('controls composer usage details from General', usagePreferences);
    await step('approves remote execution through the chat and opens an owned shell', async () => {
      await $('textarea[aria-label="Message Codex"]').setValue('Create the acceptance marker.');
      await composerInput();
      await $('button[aria-label="Send message"]').click();
      await $('.question').waitForDisplayed({ timeout: 30000 });
      const running = (await invoke<{ turn: string }>('session_snapshot', { id: session })).turn;
      await $('textarea[aria-label="Message Codex"]').setValue('Also summarize the marker after writing it.');
      await $('button[aria-label="Send follow-up"]').click();
      await expect($('textarea[aria-label="Message Codex"]')).toHaveValue('');
      expect((await invoke<{ turn: string }>('session_snapshot', { id: session })).turn).toBe(running);
      await $('textarea[aria-label="Message Codex"]').setValue('/status');
      await browser.keys('Escape');
      await browser.keys('Enter');
      await expect($('.session-status')).toHaveText(expect.stringContaining('Waiting for your input'));
      await $('[role="dialog"] button[aria-label="Close"]').click();
      await $('.question button.primary').click();
      await expect($('.messages')).toHaveText(expect.stringContaining('Created approval.txt'));
      expect((await invoke<TextFile>('file_read', { context: workspace, path: 'approval.txt' })).text).toBe(
        'from-codex\n',
      );
      await $('button[aria-label="Toggle terminal (⌘`)"]').click();
      await $('.xterm-screen').waitForDisplayed();
      expect(await browser.execute(() => document.querySelectorAll('.terminal-tab').length)).toBe(1);
      await $('.xterm-helper-textarea').addValue("sleep 0.2; printf 'hidden-ok\\n' > hidden.txt\r");
      await $('button[aria-label="Minimize terminal"]').click();
      await browser.waitUntil(async () => {
        try {
          return (
            (await invoke<TextFile>('file_read', { context: workspace, path: 'hidden.txt' })).text === 'hidden-ok\n'
          );
        } catch {
          return false;
        }
      });
      await expect($('.xterm-screen')).not.toBeDisplayed();
      await expect($('section[aria-label="Remote terminal"]')).not.toBeDisplayed();
      await $('button[aria-label="Toggle terminal (⌘`)"]').click();
      await expect($('.xterm-screen')).toBeDisplayed();
      expect(await browser.execute(() => document.querySelectorAll('.terminal-tab').length)).toBe(1);
      await $('.xterm-helper-textarea').addValue("printf 'terminal-ok\\n' > terminal.txt\r");
      await browser.waitUntil(
        async () =>
          (await invoke<TextFile>('file_read', { context: workspace, path: 'terminal.txt' })).text === 'terminal-ok\n',
      );
      await $('button=Settings').click();
      await $('button=Light').click();
      await $('[role=dialog] button[aria-label=Close]').click();
      await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/workspace-light.png'));
    });
    await step('browses server roots with zero, one or multiple workspaces and preserves navigation', async () => {
      await serverNavigation(invoke, workspace, remote, session);
    });
    await step('uses native Plan and Goal controls and reuses session ownership', async () => {
      await sessionControls(invoke, session, workspace, remote);
    });
    await step('loads conversation history with skeletons and recovers without losing navigation', async () => {
      await conversationLoading(session);
    });
    await step('applies Plan and Goal selections before the first submission', async () => {
      await draftModes(invoke, session);
    });
    await step('isolates other windows and refuses removal while this workspace is owned', async () => {
      const directory = await invoke<string>('directory_open', {
        operationId: crypto.randomUUID(),
        server: 'desktop-test',
      });
      const main = await browser.getWindowHandle();
      const label = await invoke<string>('new_window');
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 2);
      const other = (await browser.getWindowHandles()).find((id) => id !== main)!;
      await browser.switchToWindow(other);
      windowLabel = label;
      expect(
        await browser.tauri.execute(
          () => (globalThis as any).__TAURI_INTERNALS__.metadata.currentWindow.label,
          withExecuteOptions({ windowLabel }),
        ),
      ).toBe(label);
      await $('button[aria-label="Refresh workspaces"]').waitForDisplayed();
      let failure: string | undefined;
      try {
        await invoke('file_read', { context: workspace, path: 'notes.md' });
      } catch (error) {
        failure = (error as { code: string }).code;
      }
      expect(failure).toBe('RESOURCE_UNAVAILABLE');
      await expect(
        invoke('directory_browse', { operationId: crypto.randomUUID(), browser: directory, path: remote }),
      ).rejects.toMatchObject({ code: 'RESOURCE_UNAVAILABLE' });
      let removed = false;
      try {
        await invoke('workspace_remove', { server: 'desktop-test', path: remote });
        removed = true;
      } catch {}
      expect(removed).toBe(false);
      let focused: string | undefined;
      try {
        await invoke('session_open', {
          operationId: crypto.randomUUID(),
          input: { server: 'desktop-test', path: remote, resume: session, takeover: false, mcp_source: null },
        });
      } catch (error) {
        focused = (error as { code: string }).code;
      }
      expect(focused).toBe('SESSION_FOCUSED');
      await browser.tauri.execute(({ core }) => {
        setTimeout(() => {
          void core.invoke('close_window', { cancel: false });
        }, 50);
        return true;
      }, withExecuteOptions({ windowLabel }));
      windowLabel = 'main';
      await browser.switchToWindow(main);
      await invoke('directory_close', { browser: directory });
      await expect($('textarea[aria-label="Message Codex"]')).toBeDisplayed();
      await $('button=Settings').click();
      await $('button=Dark').click();
      await $('[role=dialog] button[aria-label=Close]').click();
      await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/workspace-dark.png'));
      await browser.setWindowSize(900, 600);
      await browser.saveScreenshot(path.resolve('../../.artifacts/desktop-e2e/minimum.png'));
    });
    await step('refreshes the workspace tree and manages archives independently', async () => {
      await refreshTree();
      savedUsage = (await invoke<SessionSnapshot>('session_snapshot', { id: session })).status.usage;
      await archiveAndRestore(invoke, session);
    });
    await step('starts without connecting the remembered workspace or requesting credentials', async () => {
      await startupWithoutConnection(remote, session, savedUsage);
    });
  }).timeout(600000);
});
