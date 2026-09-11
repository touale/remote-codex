import type { Channel } from '@tauri-apps/api/core';
import type {
  Choice,
  CurrentWorkspace,
  FileContext,
  Granted,
  SkippedTransfers,
  Transfer,
  WorkspaceTarget,
} from './files';
import type {
  AppEvent,
  Catalog,
  ConfigReport,
  DirectoryPage,
  HistoryPage,
  McpConfig,
  McpStatus,
  NativeStatus,
  Preferences,
  Server,
  SessionDefaults,
  SessionOpened,
  SessionSnapshot,
  TextFile,
  WindowTarget,
} from './types';

import type { AppPreferences, AppPreferencesPatch } from './preferences';
import type { AccountUsage, SessionAction, Submission } from './session';

type Command<Args, Result = void> = { args: Args; result: Result };
type Operation = { operationId: string };
type Id = { id: string };
type FilePath = { context: string; path: string };
export type AuthenticationChange =
  { action: 'password'; value: string } | { action: 'forget_password' } | { action: 'identity'; path: string | null };
export interface ServerAuthentication {
  identity: string | null;
  password_saved: boolean;
}
type FileChange =
  { action: 'directory' | 'remove'; path: string } | { action: 'rename'; path: string; destination: string };
type NativeAction =
  Exclude<SessionAction, { action: 'steer' }> | (Extract<SessionAction, { action: 'steer' }> & { client_id: string });

// The private desktop IPC contract. Callers infer results from the command name.
export interface Commands {
  attach: Command<{ channel: Channel<AppEvent> }, WindowTarget | null>;
  acknowledge: Command<Id>;
  catalog: Command<{ archived: boolean }, Catalog>;
  server_save: Command<
    Operation & {
      input: {
        name: string;
        address: string;
        port: number | null;
        identity: string | null;
        password: string | null;
        settings: [string, string][];
      };
    },
    Server
  >;
  server_remove: Command<{ name: string }>;
  server_config: Command<{ name: string; updates: [string, string][] | null; revision: number | null }, ConfigReport>;
  server_authentication: Command<
    Operation & { name: string; change: AuthenticationChange | null },
    ServerAuthentication
  >;
  workspace_open: Command<Operation & WorkspaceTarget, CurrentWorkspace>;
  workspace_remove: Command<WorkspaceTarget>;
  workspace_close: Command<Id>;
  directory_open: Command<Operation & { server: string }, string>;
  directory_browse: Command<Operation & { browser: string; path: string }, DirectoryPage>;
  directory_create: Command<Operation & { browser: string; parent: string; name: string }, string>;
  directory_close: Command<{ browser: string }>;
  file_context_open: Command<Operation & { server: string; path?: string }, FileContext>;
  file_context_close: Command<Id>;
  file_list: Command<FilePath, DirectoryPage>;
  file_read: Command<FilePath, TextFile>;
  file_write: Command<FilePath & { text: string; revision: string | null | undefined }, string>;
  file_change: Command<{ context: string; change: FileChange }>;
  project_mcp: Command<{ workspace: string; servers: McpConfig['servers'] | null; revision: string | null }, McpConfig>;
  terminal_open: Command<
    Operation & {
      target: { kind: 'server'; server: string } | { kind: 'files'; context: string };
      columns: number;
      rows: number;
    },
    CurrentWorkspace
  >;
  terminal_input: Command<Id & { bytes: number[] }>;
  terminal_resize: Command<Id & { columns: number; rows: number }>;
  terminal_close: Command<Id>;
  native_status: Command<undefined, NativeStatus>;
  native_usage: Command<undefined, AccountUsage>;
  native_select: Command<{ path: string }, NativeStatus>;
  native_login: Command<{ cancelId: string | null }, { id: string; url: string } | null>;
  session_defaults: Command<{ server: string }, SessionDefaults>;
  session_open: Command<
    Operation & { input: WorkspaceTarget & { resume: string | null; takeover: boolean; mcp_source: string | null } },
    SessionOpened
  >;
  session_trust: Command<Operation & { preparation: string; accept: boolean }, SessionOpened | null>;
  session_action: Command<Id & { action: NativeAction }, Submission | null>;
  session_revert: Command<Id & { beforeTurnId: string }, { history: HistoryPage; snapshot: SessionSnapshot }>;
  session_history: Command<Id & { cursor: string | null }, HistoryPage>;
  session_close: Command<Id>;
  session_metadata: Command<Id & { name: string | null; archived: boolean | null }>;
  session_mcp: Command<Id, McpStatus[]>;
  session_snapshot: Command<Id, SessionSnapshot>;
  session_status: Command<Id, SessionSnapshot>;
  transfer_pick: Command<{ folder: boolean }, Granted | null>;
  transfer_upload: Command<{ context: string; destination: string; token: string }, Transfer>;
  transfer_download: Command<{ context: string; source: string }, Transfer | null>;
  transfer_discard_grant: Command<{ token: string }>;
  transfer_remove: Command<{ ids: string[] }, string[]>;
  transfer_list: Command<undefined, Transfer[]>;
  transfer_run: Command<Id>;
  transfer_action: Command<
    Id & { action: { type: 'pause' | 'cancel' | 'restart' } | { type: 'resolve'; choice: Choice; all: boolean } }
  >;
  transfer_skipped: Command<Id & { after: number | null }, SkippedTransfers>;
  transfer_reveal: Command<Id>;
  preferences: Command<{ value: Preferences | null }, Preferences>;
  app_preferences: Command<{ patch: AppPreferencesPatch | null }, AppPreferences>;
  authentication_answer: Command<Id & { answer: string | null }>;
  cancel_operation: Command<Id>;
  new_window: Command<{ target: WindowTarget | null }, string>;
  close_window: Command<{ cancel: boolean }>;
  external_link: Command<{ url: string }>;
}
