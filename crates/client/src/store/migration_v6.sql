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
