import type { CurrentWorkspace, WorkspaceTarget } from '../bridge/files';

export type NavigationLocation =
  | { kind: 'home' }
  | { kind: 'server'; serverId: string }
  | { kind: 'directory'; target: WorkspaceTarget }
  | { kind: 'workspace'; workspace: CurrentWorkspace }
  | { kind: 'draft'; key: string; target: WorkspaceTarget }
  | { kind: 'session'; id: string; workspace: CurrentWorkspace };
export function locationTarget(location: NavigationLocation): WorkspaceTarget | null {
  if (location.kind === 'draft' || location.kind === 'directory') return location.target;
  if (location.kind === 'workspace' || location.kind === 'session') return location.workspace;
  return null;
}
