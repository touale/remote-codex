-- Frozen schema 6 import fixture; independent of current production DDL.
CREATE TABLE installation (id TEXT NOT NULL PRIMARY KEY);
CREATE TABLE server_revisions (
    id TEXT NOT NULL PRIMARY KEY,
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
);
CREATE TABLE connections (
    id TEXT NOT NULL PRIMARY KEY REFERENCES server_revisions(id),
    name TEXT NOT NULL UNIQUE,
    endpoint TEXT NOT NULL,
    phase TEXT NOT NULL DEFAULT 'saved',
    runtime TEXT
);
CREATE INDEX connection_endpoint ON connections(endpoint);
CREATE TABLE settings (
    server TEXT NOT NULL REFERENCES server_revisions(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    spelling TEXT NOT NULL,
    representation TEXT NOT NULL CHECK (representation IN ('plain', 'secret')),
    value TEXT NOT NULL,
    PRIMARY KEY(server, key)
);
CREATE TABLE credentials (
    id TEXT NOT NULL PRIMARY KEY,
    state TEXT NOT NULL CHECK (state IN ('pending','active','retired')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE server_access(
    server TEXT PRIMARY KEY REFERENCES connections(id) ON DELETE CASCADE,
    identity_file TEXT, managed_public_key TEXT, ssh_config TEXT,
    remote_identity TEXT, service_executable TEXT, service_root TEXT,
    applied_revision INTEGER, checked_at INTEGER, health TEXT NOT NULL DEFAULT 'unknown'
);
CREATE TABLE workspace_history(
    server TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    path TEXT NOT NULL,used_at INTEGER NOT NULL DEFAULT(unixepoch()),PRIMARY KEY(server,path)
);
DELETE FROM settings WHERE key='codex.home';
CREATE TABLE IF NOT EXISTS retained_legacy_credentials(id TEXT PRIMARY KEY);
INSERT OR IGNORE INTO retained_legacy_credentials
    SELECT value FROM settings WHERE representation='secret' AND upper(key) IN
    ('ENV.OPENAI_API_KEY','ENV.CODEX_API_KEY','ENV.CODEX_ACCESS_TOKEN','ENV.CODEX_AUTH_JSON');
-- Credentials remain in the original vault and backup; these old remote-Agent
-- bindings cannot be applied to an execution-only environment.
DELETE FROM settings WHERE upper(key) IN ('ENV.OPENAI_API_KEY','ENV.CODEX_API_KEY','ENV.CODEX_ACCESS_TOKEN','ENV.CODEX_AUTH_JSON');
UPDATE server_access SET service_executable=NULL,service_root=NULL,remote_identity=NULL,
    applied_revision=NULL,checked_at=NULL,health='needs_execution_setup';
DROP TABLE IF EXISTS session_cache;
DROP TABLE IF EXISTS server_histories;
CREATE TABLE IF NOT EXISTS local_sessions (
    id TEXT PRIMARY KEY,
    server TEXT NOT NULL REFERENCES connections(id),
    record TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE INDEX IF NOT EXISTS local_sessions_server ON local_sessions(server,updated_at);
CREATE TABLE IF NOT EXISTS project_trust (
    server TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    path TEXT NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(server,path)
);

DROP TABLE IF EXISTS login_operations;
DROP TABLE IF EXISTS login_garbage;
DROP TABLE IF EXISTS logins;
