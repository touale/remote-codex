import type { Environment, Progress } from '../bridge/types';

const stages: Record<string, string> = {
  connect_ssh: 'Connecting to SSH',
  inspect_host: 'Checking remote host',
  inspect_runtime: 'Checking execution runtime',
  inspect_cache: 'Checking download cache',
  use_cached_package: 'Using cached package',
  download: 'Downloading execution runtime',
  verify_download: 'Verifying download',
  upload: 'Uploading execution runtime',
  verify_install: 'Verifying installation',
  prepared: 'Environment prepared',
  install_service: 'Installing execution service',
  start_service: 'Starting execution service',
  use_running_service: 'Using running service',
  synchronize: 'Synchronizing server settings',
  start_local_codex: 'Starting local Codex',
  connect_execution: 'Connecting execution environment',
  prepare_skills: 'Preparing enabled Skills',
  open_local_session: 'Opening local session and MCP servers',
};

export function operationStatus(event: Progress): { label: string; percent: number | null } {
  if (event.type === 'stage') return { label: stages[event.data] ?? event.data, percent: null };
  const { kind, total_bytes: total, transferred_bytes: transferred } = event.data;
  const percent = total && total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : null;
  const size = `${(transferred / 1048576).toFixed(1)}${total && total > 0 ? ` / ${(total / 1048576).toFixed(1)}` : ''} MiB`;
  return {
    label: `${kind === 'upload' ? 'Uploading' : 'Downloading'} · ${size}${percent === null ? '' : ` · ${percent}%`}`,
    percent,
  };
}

export interface Status {
  label: string;
  tone: 'idle' | 'busy' | 'online' | 'attention';
}

export function contextStatus(input: {
  ready: boolean;
  busy: boolean;
  workspace: boolean;
  serverFiles?: boolean;
  error: string;
  closed?: boolean;
  environment?: Environment;
}): Status {
  if (input.busy) return { label: 'Connecting…', tone: 'busy' };
  if (input.error) return { label: 'Attention required', tone: 'attention' };
  if (!input.ready) return { label: 'Starting…', tone: 'busy' };
  if (input.closed) return { label: 'Session closed', tone: 'idle' };
  switch (input.environment?.status) {
    case 'ready':
      return { label: 'Ready', tone: 'online' };
    case 'reconnecting':
      return {
        label: `Reconnecting · attempt ${input.environment.attempt}/${input.environment.max_attempts}${input.environment.retry_in_ms > 0 ? ' · retry in 5s' : ''}`,
        tone: 'busy',
      };
    case 'recovering':
      return { label: 'Restoring execution environment…', tone: 'busy' };
    case 'action_required':
      return { label: 'Connection requires attention', tone: 'attention' };
    case 'closed':
      return { label: 'Disconnected', tone: 'idle' };
    default:
      return {
        label: input.workspace ? 'Workspace open' : input.serverFiles ? 'Server files open' : 'No workspace selected',
        tone: 'idle',
      };
  }
}
