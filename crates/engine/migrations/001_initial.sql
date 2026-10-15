BEGIN IMMEDIATE;
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    source BLOB NOT NULL,
    display_url TEXT NOT NULL,
    destination TEXT NOT NULL UNIQUE,
    staging TEXT NOT NULL UNIQUE,
    replayable INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'queued',
    generation INTEGER NOT NULL DEFAULT 0,
    total INTEGER,
    etag TEXT,
    resolved_url BLOB,
    mode TEXT NOT NULL DEFAULT 'sequential',
    durable INTEGER NOT NULL DEFAULT 0,
    expected_sha256 TEXT,
    actual_sha256 TEXT,
    error TEXT,
    requested TEXT NOT NULL DEFAULT 'run',
    publication_file_id TEXT,
    created INTEGER NOT NULL
);
CREATE TABLE extents (
    job_id TEXT NOT NULL REFERENCES jobs(id),
    generation INTEGER NOT NULL,
    offset INTEGER NOT NULL,
    length INTEGER NOT NULL,
    digest TEXT NOT NULL,
    PRIMARY KEY(job_id, offset)
);
PRAGMA user_version=1;
COMMIT;
