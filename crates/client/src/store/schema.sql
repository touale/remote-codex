CREATE TABLE installation (id TEXT NOT NULL PRIMARY KEY);
CREATE TABLE server_revisions (
    id TEXT NOT NULL PRIMARY KEY,
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
);
CREATE TABLE connections (
    id TEXT NOT NULL PRIMARY KEY REFERENCES server_revisions(id),
    name TEXT NOT NULL UNIQUE,
    endpoint TEXT NOT NULL,
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

CREATE TABLE local_sessions (
    id TEXT PRIMARY KEY,
    server TEXT NOT NULL REFERENCES connections(id),
    record TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE INDEX local_sessions_server ON local_sessions(server,updated_at);
CREATE TABLE project_trust (
    server TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    path TEXT NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(server,path)
);
