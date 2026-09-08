CREATE TABLE installation (id TEXT NOT NULL PRIMARY KEY);
CREATE TABLE scopes (
    id TEXT NOT NULL PRIMARY KEY,
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
);
INSERT INTO scopes(id) VALUES ('global');
CREATE TABLE connections (
    id TEXT NOT NULL PRIMARY KEY REFERENCES scopes(id),
    name TEXT NOT NULL UNIQUE,
    endpoint TEXT NOT NULL,
    phase TEXT NOT NULL DEFAULT 'saved',
    runtime TEXT
);
CREATE INDEX connection_endpoint ON connections(endpoint);
CREATE TABLE settings (
    scope TEXT NOT NULL REFERENCES scopes(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    spelling TEXT NOT NULL,
    representation TEXT NOT NULL CHECK (representation IN ('plain', 'secret')),
    value TEXT NOT NULL,
    PRIMARY KEY(scope, key)
);
CREATE TABLE contexts (
    id TEXT NOT NULL PRIMARY KEY,
    connection_id TEXT REFERENCES connections(id) ON DELETE SET NULL,
    workspace TEXT
);
CREATE TABLE credentials (
    id TEXT NOT NULL PRIMARY KEY,
    state TEXT NOT NULL CHECK (state IN ('pending','active','retired')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
