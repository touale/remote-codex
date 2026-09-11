import type { WorkspaceTarget } from '../bridge/files';
import type { Server, Workspace, WindowTarget } from '../bridge/types';
import type { ChatSummary } from '../chat/store';

export interface TreeActions {
  onSelectServer: (server: Server) => void;
  onSelectDirectory: (target: WorkspaceTarget) => void;
  onSelectWorkspace: (workspace: Workspace) => void;
  onSelectSession: (server: string, session: string, path: string) => void;
  onNew: (workspace: Workspace) => void;
  onRefresh: () => void;
  refreshing: boolean;
  onAddServer: () => void;
  onEditServer: (server: Server) => void;
  onRemoveServer: (server: Server) => void;
  onAddWorkspace: (server: Server) => void;
  onRemoveWorkspace: (workspace: Workspace) => void;
  onSessionMenu: (id: string, action: 'rename' | 'archive' | 'close') => void;
  onTerminal: (workspace: Workspace) => void;
  onWindow: (target: WindowTarget) => void;
  report: (error: unknown) => void;
}
export interface TreeSelection {
  openSessions: string[];
  serverHome: string | null;
  chats: Record<string, ChatSummary>;
  selected: string | null;
  current: { server: string; path: string } | null;
}
export interface TreeContext {
  selection: TreeSelection;
  actions: TreeActions;
  collapsed: Set<string>;
  toggle: (id: string, expand?: boolean) => void;
  searching: boolean;
}
export const copyTreeText = (text: string, actions: TreeActions) =>
  void navigator.clipboard.writeText(text).catch(actions.report);
