import { Channel } from '@tauri-apps/api/core';
import { call } from './client';

export type UpdateMode = 'notify' | 'auto' | 'manual';
export type RestartResult =
  { status: 'confirmation_required'; sessions: number; terminals: number; transfers: number } | { status: 'closing' };
export interface UpdateSnapshot {
  mode: UpdateMode;
  last_checked: number | null;
  latest: { version: string; url: string } | null;
  current_version: string;
  installed_version: string;
  path: string;
  can_install: boolean;
  busy: boolean;
  available: boolean;
  restart_required: boolean;
}

export interface UpdateProgress {
  phase: string;
  received: number;
  total: number;
}

export function installUpdate(onProgress: (progress: UpdateProgress) => void): Promise<UpdateSnapshot> {
  const channel = new Channel<UpdateProgress>();
  channel.onmessage = onProgress;
  return call('update_install', { channel });
}
