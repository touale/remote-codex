export interface WorkspaceTarget {
  server: string;
  path: string;
}
export interface CurrentWorkspace extends WorkspaceTarget {
  id: string;
}
export interface FileContext extends CurrentWorkspace {
  kind: 'server' | 'workspace';
}
export type Choice = 'replace' | 'keep_both' | 'skip' | 'merge';
export interface Transfer {
  id: string;
  name: string;
  server: string;
  workspace: string;
  direction: 'upload' | 'download';
  destination: string;
  status: 'queued' | 'paused' | 'preparing' | 'running' | 'reconnecting' | 'conflict' | 'completed' | 'cancelled';
  bytes: number;
  total: number;
  files: number;
  completed: number;
  skipped: number;
  message: string | null;
  error_code: string | null;
  conflict: { path: string; source: string; destination: string } | null;
  active: boolean;
  owned: boolean;
}
export interface Granted {
  token: string;
  names: string[];
}

export interface SkippedTransfers {
  paths: string[];
  next: number | null;
}
