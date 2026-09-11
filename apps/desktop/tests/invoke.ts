import { browser } from '@wdio/globals';
import { withExecuteOptions } from '@wdio/tauri-service';

type Result = { ok: true; value: unknown } | { ok: false; error: unknown };
declare global {
  interface Window {
    __remoteCodexTestRequests?: Record<string, { result?: Result }>;
  }
}

export async function invokeWindow<T>(
  windowLabel: string,
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const id = crypto.randomUUID();
  const target = withExecuteOptions({ windowLabel });
  // A long direct eval can block WebKit from delivering subsequent native IPC.
  // Dispatch once, then observe its result without holding that eval open.
  await browser.tauri.execute(
    ({ core }, command, args, id) => {
      const requests = (window.__remoteCodexTestRequests ??= {});
      if (requests[id]) return;
      const entry: { result?: Result } = {};
      requests[id] = entry;
      void Promise.resolve()
        .then(() => core.invoke(command, args))
        .then(
          (value) => {
            entry.result = { ok: true, value };
          },
          (error) => {
            entry.result = { ok: false, error };
          },
        );
    },
    target,
    command,
    args,
    id,
  );
  const response: { value: Result | null } = { value: null };
  try {
    await browser.waitUntil(
      async () => {
        response.value = await browser.tauri.execute(
          (_, id) => window.__remoteCodexTestRequests?.[id]?.result ?? null,
          target,
          id,
        );
        return response.value !== null;
      },
      { timeout: 120000, interval: 50, timeoutMsg: `${command} did not complete in ${windowLabel}` },
    );
    const completed = response.value;
    if (!completed) throw new Error(`${command} returned no result`);
    if (!completed.ok) {
      const issue = completed.error as { message?: string };
      throw Object.assign(new Error(`${command}: ${issue?.message ?? JSON.stringify(issue)}`), issue);
    }
    return completed.value as T;
  } finally {
    await browser.tauri
      .execute(
        (_, id) => {
          delete window.__remoteCodexTestRequests?.[id];
        },
        target,
        id,
      )
      .catch(() => {});
  }
}
