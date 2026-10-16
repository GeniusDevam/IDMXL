BEGIN IMMEDIATE;
CREATE TABLE queue_windows (
    queue_name TEXT PRIMARY KEY,
    schedule TEXT NOT NULL
);
CREATE INDEX jobs_destination_lookup ON jobs(destination COLLATE NOCASE);
PRAGMA user_version=4;
COMMIT;
