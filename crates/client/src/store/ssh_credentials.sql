CREATE TABLE ssh_credentials (
    id TEXT PRIMARY KEY,
    server TEXT NOT NULL,
    target TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('pending','active','retired')),
    created_at INTEGER NOT NULL DEFAULT(unixepoch())
);
CREATE UNIQUE INDEX ssh_credentials_active ON ssh_credentials(server) WHERE state='active';
