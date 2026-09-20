import type { RateLimit, SessionStatus, TokenUsage, TurnState, TurnTiming } from './session';
export interface Failure {
  code: string;
  message: string;
  outcome_unknown?: boolean;
}
export interface Server {
  id: string;
  name: string;
  endpoint: { host: string; user: string | null; port: number | null };
}
export interface Workspace {
  server_id: string;
  server: string;
  path: string;
  used_at: number;
}
export interface Session {
  id: string;
  title: string;
  cwd: string;
  created_at: number;
  updated_at: number;
  archived: boolean;
  state: string;
}
export interface CachedSession {
  server_id: string;
  server: string;
  session: Session;
}
export interface Settings {
  mode: 'agent' | 'plan';
  model: string;
  effort: string | null;
  full_access: boolean;
  approval_policy: string;
  reviewer: string;
}
export interface Model {
  id: string;
  name: string;
  description: string;
  efforts: string[];
  default_effort: string;
}
export interface SessionDefaults {
  settings: Settings;
  models: Model[];
}
interface LiveSession {
  session: Session;
  server: string;
  settings: Settings;
}
export type WindowTarget =
  | import('./editor').ContentTarget
  | { kind: 'server'; server: string }
  | { kind: 'workspace'; server: string; path: string }
  | { kind: 'session'; id: string };
export interface Catalog {
  open_session_ids: string[];
  servers: Server[];
  workspaces: Workspace[];
  sessions: CachedSession[];
  live: LiveSession[];
}
export interface Entry {
  name: string;
  path: string;
  directory: boolean;
  symlink: boolean;
  size: number;
}
export interface DirectoryPage {
  entries: Entry[];
  truncated: boolean;
}
export interface TextFile {
  path: string;
  text: string;
  revision: string;
}
export interface ToolItem {
  id: string;
  kind: string;
  title: string;
  output: string;
  status: string;
  changes: { path: string; diff: string }[];
  input?: string;
  links?: { title: string; url: string; description?: string }[];
}
interface InputField {
  id: string;
  label: string;
  description: string;
  kind: string;
  required: boolean;
  choices: string[];
  secret: boolean;
}
export type Interaction =
  | { kind: 'questions'; fields: InputField[] }
  | { kind: 'mcp_form'; fields: InputField[]; server: string; message: string }
  | { kind: 'mcp_url'; server: string; message: string; url: string }
  | { kind: 'unsupported'; message: string };
export type Environment =
  | { status: 'ready' | 'closed' }
  | { status: 'reconnecting'; attempt: number; max_attempts: number; retry_in_ms: number }
  | { status: 'recovering'; reason: string }
  | { status: 'action_required'; code: string; message: string };
export interface Goal {
  thread_id: string;
  objective: string;
  status: 'active' | 'paused' | 'blocked' | 'usage_limited' | 'budget_limited' | 'complete';
  token_budget: number | null;
  tokens_used: number;
  time_used_seconds: number;
}
export interface Plan {
  turn: string;
  explanation: string | null;
  steps: { step: string; status: string }[];
}
export interface AsyncQuestion {
  title: string;
  options: string[];
}
export type SessionEvent =
  | { type: 'session_updated'; session: Session }
  | { type: 'activity_changed'; activity: string; active_flags: string[] }
  | { type: 'usage_changed'; usage: TokenUsage }
  | { type: 'rate_limits_changed'; limits: RateLimit[] }
  | { type: 'user_message'; item_id: string; client_id: string | null; turn_id: string; text: string }
  | { type: 'goal_changed'; goal: Goal | null }
  | { type: 'plan_changed'; plan: Plan }
  | { type: 'plan_message'; item_id: string; turn_id: string; text: string; complete: boolean }
  | {
      type: 'message';
      item_id: string;
      turn_id: string;
      phase: string | null;
      text: string;
      complete: boolean;
      delivery?: string | null;
      questions?: AsyncQuestion[];
    }
  | { type: 'tool_output'; item_id: string; turn_id: string; text: string }
  | { type: 'tool_changed'; item: ToolItem; turn_id: string }
  | { type: 'approval_requested'; request_id: string; description: string }
  | { type: 'interaction_requested'; request_id: string; interaction: Interaction }
  | { type: 'interaction_required'; request_id: string; kind: string }
  | { type: 'interaction_resolved'; request_id: string }
  | { type: 'turn_started'; id: string; timing: TurnTiming }
  | { type: 'turn_completed'; id: string; timing: TurnTiming; outcome: { status: string; message?: string } }
  | { type: 'settings_changed'; settings: Settings }
  | { type: 'environment_changed'; state: Environment }
  | { type: 'warning'; message: string }
  | { type: 'closed'; reason: string | null };
export interface AuthPrompt {
  target: string;
  kind: 'host_key' | 'password' | 'passphrase';
  message: string;
}
export type ShellEvent = { type: 'output'; bytes: number[] } | { type: 'closed'; code: number | null };
export type Progress =
  | { type: 'stage'; data: string }
  | { type: 'transfer'; data: { kind: string; transferred_bytes: number; total_bytes: number | null } };
export type AppEvent =
  | { kind: 'transfers_removed'; ids: string[] }
  | { kind: 'file_relocated'; server: string; source: string; destination: string; context: string }
  | { kind: 'transfer'; transfer: import('./files').Transfer }
  | { kind: 'files_dropped'; token: string; names: string[]; x: number; y: number }
  | { kind: 'app_preferences_changed'; preferences: import('./preferences').AppPreferences }
  | { kind: 'focus_session'; id: string }
  | { kind: 'ui_action'; action: string }
  | { kind: 'delivery'; id: string; event: AppEvent }
  | { kind: 'authentication_ended'; id: string }
  | { kind: 'operation_finished'; id: string }
  | { kind: 'session'; id: string; event: SessionEvent }
  | { kind: 'terminal'; id: string; event: ShellEvent }
  | { kind: 'progress'; operation: string; event: Progress }
  | { kind: 'authentication'; id: string; prompt: AuthPrompt }
  | { kind: 'notice'; message: string }
  | { kind: 'catalog_changed' | 'close_requested' }
  | { kind: 'resync'; id: string };
export interface Preferences {
  sidebar_width: number;
  editor_width: number;
  terminal_height: number;
  tree_split: number;
  codex_program: string | null;
  selected_workspace: [string, string] | null;
  sidebar_visible: boolean;
  editor_visible: boolean;
  terminal_visible: boolean;
  workspaces_collapsed: boolean;
  files_collapsed: boolean;
  collapsed_nodes: string[];
}
export interface NativeStatus {
  program: string;
  version: string;
  account: { logged_in: boolean; email: string | null; plan: string | null };
}
export type SessionOpened =
  | { status: 'open'; session: Session; settings: Settings; models: Model[]; snapshot: SessionSnapshot }
  | { status: 'trust'; preparation: string; server: string; path: string; names: string[] };
export interface HistoryPage {
  session: Session;
  turns: {
    id: string;
    status: string;
    timing: TurnTiming;
    items: {
      id: string;
      kind: string;
      text: string;
      tool?: ToolItem;
      client_id: string | null;
      phase: string | null;
      sent_at: number | null;
      delivery?: string | null;
      questions?: AsyncQuestion[];
    }[];
  }[];
  next_cursor: string | null;
}
export interface McpConfig {
  revision: string | null;
  servers: { name: string; definition: string }[];
}
export interface McpStatus {
  name: string;
  status: string;
  authentication: string;
  tools: number;
}
export interface ConfigReport {
  saved_revision: number;
  applied_revision: number | null;
  items: { key: string; value: unknown; application_state: string }[];
}

export interface SessionSnapshot {
  status: SessionStatus;
  current_turn: TurnState | null;
  goal: Goal | null;
  plan: Plan | null;
  settings: Settings;
  environment: Environment;
  turn: string | null;
  pending: SessionEvent[];
  closed: boolean;
}
