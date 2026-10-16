BEGIN IMMEDIATE;
CREATE TABLE request_attempts (
 job_id TEXT NOT NULL REFERENCES jobs(id), generation INTEGER NOT NULL,
 offset INTEGER NOT NULL, length INTEGER NOT NULL,
 started INTEGER NOT NULL DEFAULT 0, budget_failures INTEGER NOT NULL DEFAULT 0,
 total_failures INTEGER NOT NULL DEFAULT 0,
 PRIMARY KEY(job_id,generation,offset,length)
);
CREATE TABLE completion_events (
 job_id TEXT PRIMARY KEY REFERENCES jobs(id), created INTEGER NOT NULL,
 delivered INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO settings VALUES ('notify_completed','false');
PRAGMA user_version=6;
COMMIT;
