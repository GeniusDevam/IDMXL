BEGIN IMMEDIATE;
ALTER TABLE jobs ADD COLUMN received INTEGER NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN speed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN eta INTEGER;
ALTER TABLE jobs ADD COLUMN public_only INTEGER NOT NULL DEFAULT 0;
UPDATE jobs SET received=durable;
CREATE TABLE policies (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id),
    queue_name TEXT NOT NULL DEFAULT 'Default',
    priority INTEGER NOT NULL DEFAULT 0,
    not_before INTEGER NOT NULL DEFAULT 0,
    deadline INTEGER,
    connections INTEGER NOT NULL DEFAULT 2,
    retries INTEGER NOT NULL DEFAULT 2,
    speed_limit INTEGER NOT NULL DEFAULT 0
);
INSERT INTO policies(job_id) SELECT id FROM jobs;
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO settings VALUES ('queue_running','false'),('global_speed_limit','0');
CREATE TABLE handoffs (request_id TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES jobs(id));
PRAGMA user_version=2;
COMMIT;
