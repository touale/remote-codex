import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Channel, invoke } from '@tauri-apps/api/core';
import type { Commands } from './commands';
import type { AppEvent, Failure } from './types';

const listeners = new Set<(event: AppEvent) => void | Promise<void>>();
let attachment: Promise<[string, string] | null> | undefined;
export function attach(): Promise<[string, string] | null> {
  if (!attachment) {
    const channel = new Channel<AppEvent>();
    channel.onmessage = (message) => {
      const event = message.kind === 'delivery' ? message.event : message;
      void Promise.all([...listeners].map((listener) => listener(event))).finally(() => {
        if (message.kind === 'delivery') void call('acknowledge', { id: message.id }).catch(() => {});
      });
    };
    attachment = call('attach', { channel }).catch((error) => {
      attachment = undefined;
      throw error;
    });
  }
  return attachment;
}
export function listen(listener: (event: AppEvent) => void | Promise<void>): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
export async function call<K extends keyof Commands>(
  command: K,
  ...args: Commands[K]['args'] extends undefined ? [args?: undefined] : [args: Commands[K]['args']]
): Promise<Commands[K]['result']> {
  try {
    return await invoke<Commands[K]['result']>(command, args[0] && { ...args[0] });
  } catch (error) {
    throw failure(error);
  }
}
export function failure(error: unknown): Failure {
  if (typeof error === 'object' && error && 'message' in error) {
    return {
      code: 'code' in error ? String(error.code) : 'ERROR',
      message: String(error.message),
      outcome_unknown: 'outcome_unknown' in error && Boolean(error.outcome_unknown),
    };
  }
  return { code: 'ERROR', message: typeof error === 'string' ? error : 'The operation could not be completed.' };
}
export const operationId = () => crypto.randomUUID();
export const openLink = (url: string) => call('external_link', { url });

export function onFileDrag(listener: (position: { x: number; y: number } | null) => void) {
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === 'enter' || payload.type === 'over') listener(payload.position);
    else if (payload.type === 'leave') listener(null);
  });
}
