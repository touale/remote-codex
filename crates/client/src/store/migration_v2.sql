CREATE TABLE server_access(
    server TEXT PRIMARY KEY REFERENCES connections(id) ON DELETE CASCADE,
    identity_file TEXT, managed_public_key TEXT, ssh_config TEXT,
    remote_identity TEXT, service_executable TEXT, service_root TEXT,
    applied_revision INTEGER, checked_at INTEGER, health TEXT NOT NULL DEFAULT 'unknown'
);
INSERT INTO server_access(server) SELECT id FROM connections;
CREATE TABLE workspace_history(
    server TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    path TEXT NOT NULL,used_at INTEGER NOT NULL DEFAULT(unixepoch()),PRIMARY KEY(server,path)
);
INSERT OR IGNORE INTO workspace_history(server,path)
    SELECT scope,value FROM settings WHERE key='workspace' AND scope!='global' AND representation='plain';
CREATE TABLE session_cache(
    remote_identity TEXT NOT NULL,history_home TEXT NOT NULL,id TEXT NOT NULL,
    record TEXT NOT NULL,checked_at INTEGER NOT NULL DEFAULT(unixepoch()),
    PRIMARY KEY(remote_identity,history_home,id)
);
CREATE TABLE server_histories(
    server TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
    remote_identity TEXT NOT NULL,history_home TEXT NOT NULL,
    PRIMARY KEY(server,remote_identity,history_home)
);
