CREATE TABLE identity (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    id TEXT NOT NULL
);

CREATE TABLE profiles (
    id TEXT PRIMARY KEY,
    config TEXT NOT NULL,
    revision INTEGER NOT NULL
);

CREATE TABLE operations (
    id TEXT PRIMARY KEY,
    digest TEXT NOT NULL,
    result TEXT,
    event_cursor INTEGER
);

CREATE TABLE execution_channels (
    id TEXT PRIMARY KEY,
    profile TEXT NOT NULL,
    revision INTEGER NOT NULL,
    state TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT(unixepoch()),
    replay_bytes INTEGER NOT NULL DEFAULT 0,
    replay_floor INTEGER NOT NULL DEFAULT 0,
    service_instance TEXT NOT NULL DEFAULT '',
    boot_id TEXT NOT NULL DEFAULT '',
    lost_reason TEXT
);

CREATE TABLE exec_events (
    cursor INTEGER PRIMARY KEY AUTOINCREMENT,
    channel TEXT NOT NULL,
    record TEXT NOT NULL
);

CREATE INDEX exec_events_channel ON exec_events(channel,cursor);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    process_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    profile TEXT NOT NULL,
    channel TEXT NOT NULL,
    thread TEXT,
    cwd TEXT NOT NULL,
    state TEXT NOT NULL,
    exit_code INTEGER,
    created_at INTEGER NOT NULL DEFAULT(unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT(unixepoch()),
    output_bytes INTEGER NOT NULL DEFAULT 0,
    output_truncated INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX jobs_profile ON jobs(profile,thread,created_at);

CREATE TABLE job_output (
    cursor INTEGER PRIMARY KEY AUTOINCREMENT,
    job TEXT NOT NULL,
    record TEXT NOT NULL
);

CREATE INDEX job_output_job ON job_output(job,cursor);

CREATE INDEX operations_event ON operations(event_cursor);

CREATE INDEX operations_channel ON operations(substr(id,1,instr(id,'/')-1));

CREATE INDEX jobs_process ON jobs(channel,process_id);

CREATE TABLE execution_approvals (
    id TEXT PRIMARY KEY,
    channel TEXT NOT NULL,
    thread TEXT NOT NULL,
    turn TEXT NOT NULL,
    item TEXT NOT NULL,
    argv TEXT NOT NULL,
    cwd TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT(unixepoch())
);

CREATE TABLE session_permissions (
    id TEXT PRIMARY KEY,
    channel TEXT NOT NULL,
    thread TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT(unixepoch())
);
