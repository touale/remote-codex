import { $, expect } from '@wdio/globals';
import { readFileSync, unlinkSync, writeFileSync } from 'node:fs';
import type { Catalog } from '../src/bridge/types';
import { contextAt } from './interaction-controls';
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
function events(kind: string) {
  const log = readFileSync(process.env.REMOTE_CODEX_ACCEPTANCE_RELAYS!, 'utf8');
  return log
    .slice(0, log.lastIndexOf('\n'))
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .filter((entry) => entry.kind === kind);
}
export const masters = () => events('master');
export async function coldDirectoryBrowsing(invoke: Invoke, remote: string) {
  expect((await invoke<Catalog>('catalog', { archived: false })).workspaces).toHaveLength(0);
  const before = masters().length;
  const authenticationBefore = events('authentication').length;
  const challenge = process.env.REMOTE_CODEX_E2E_AUTH_CHALLENGE!;
  writeFileSync(challenge, 'fixture-only');
  await contextAt('.server-row .tree-label');
  await $('[role="menuitem"]=Add workspace…').click();
  const location = $('.workspace-dialog input[placeholder="/workspace/project"]');
  await location.setValue(`${remote}/missing`);
  await $('button=Browse').click();
  await authenticate();
  await expect($('.workspace-dialog [role="alert"]')).toBeDisplayed();
  await location.setValue(remote);
  await $('button=Browse').click();
  await $('button=New folder').waitForEnabled();
  const master = masters().at(-1)!;
  expect(masters().length - before).toBe(1);
  expect(events('authentication').length - authenticationBefore).toBe(1);
  for (const name of ['cold-a', 'cold-b']) {
    await $('button=New folder').click();
    await $('.new-workspace-folder input').setValue(name);
    await $('button=Create folder').click();
    await expect(location).toHaveValue(`${remote}/${name}`);
    await $('button=Parent directory').waitForEnabled();
    await $('button=Parent directory').click();
    await $('button=New folder').waitForEnabled();
  }
  expect(masters().length - before).toBe(1);
  expect(events('authentication').length - authenticationBefore).toBe(1);
  // Drop the owned local master, keeping the external SSH fixture only for remote I/O.
  process.kill(master.pid, 'SIGTERM');
  await location.setValue(remote);
  await $('button=Browse').click();
  await authenticate();
  await $('button=New folder').waitForEnabled();
  expect(masters().length - before).toBe(2);
  await $('button=Open workspace').click();
  await $('.workspace-dialog').waitForExist({ reverse: true });
  await expect($('.workspace-path')).toHaveText(remote);
  expect(masters().length - before).toBe(2);
  expect(events('authentication').length - authenticationBefore).toBe(2);
  unlinkSync(challenge);
  writeFileSync(
    new URL('../../../.artifacts/desktop-e2e/directory-connections.json', import.meta.url),
    JSON.stringify(
      {
        authentication_prompts: events('authentication').length - authenticationBefore,
        initial_master_count: 1,
        after_navigation_and_creation: 1,
        after_forced_disconnect: 2,
        after_workspace_open: masters().length - before,
      },
      null,
      2,
    ),
  );
}

async function authenticate() {
  const input = $('[role="dialog"] input[type="password"]');
  await input.waitForDisplayed();
  await input.setValue('fixture-password');
  await $('button=Connect').click();
  await input.waitForExist({ reverse: true });
}
