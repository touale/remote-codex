import { browser } from '@wdio/globals';
import type { FileContext } from '../src/bridge/files';
type Command = 'file_list' | 'file_read' | 'file_context_open';
interface Held {
  command: Command;
  path: string;
  release: (fail: boolean) => void;
  released: boolean;
  settled: boolean;
  context?: FileContext;
}
declare global {
  interface Window {
    fileGate: { held: Held[]; restore: () => void };
  }
}

// Delay native file responses, or a selected server connection before it opens.
export async function holdFiles(server?: string) {
  await browser.execute((server) => {
    const original = window.fetch;
    const gate: Window['fileGate'] = {
      held: [],
      restore: () => {
        window.fetch = original;
      },
    };
    window.fileGate = gate;
    window.fetch = async (input, init) => {
      const url = new URL(
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
        location.href,
      );
      const args = typeof init?.body === 'string' ? JSON.parse(init.body) : null;
      const command = url.pathname.slice(1) as Command;
      const matches = server
        ? command === 'file_context_open' && args?.server === server
        : command === 'file_list' || command === 'file_read';
      if (url.protocol !== 'ipc:' || !matches) return original.call(window, input, init);
      const response = server ? undefined : await original.call(window, input, init);
      let request!: Held;
      const fail = await new Promise<boolean>((release) => {
        request = { command, path: args.path ?? args.server, release, released: false, settled: false };
        gate.held.push(request);
      });
      const result = fail
        ? new Response(JSON.stringify({ code: 'SSH_FAILED', message: 'Fixture file operation interrupted.' }), {
            headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' },
          })
        : (response ?? (await original.call(window, input, init)));
      if (command === 'file_context_open' && !fail) request.context = await result.clone().json();
      request.settled = true;
      return result;
    };
  }, server ?? null);
}
export async function heldFile(command: Command, path: string, after = -1) {
  let id = -1;
  await browser.waitUntil(async () => {
    id = await browser.execute(
      (command, path, after) =>
        window.fileGate.held.findIndex(
          (request, index) =>
            index > after && !request.released && request.command === command && request.path === path,
        ),
      command,
      path,
      after,
    );
    return id >= 0;
  });
  return id;
}
export async function releaseFile(id: number, fail = false) {
  await browser.execute(
    (id, fail) => {
      const request = window.fileGate.held[id];
      request.released = true;
      request.release(fail);
    },
    id,
    fail,
  );
  await browser.executeAsync((done) => requestAnimationFrame(() => requestAnimationFrame(() => done())));
}
export async function restoreFiles() {
  await browser.execute(() => {
    window.fileGate.restore();
    window.fileGate.held.filter((request) => !request.released).forEach((request) => request.release(false));
  });
}
