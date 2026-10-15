BEGIN IMMEDIATE;
ALTER TABLE jobs ADD COLUMN last_started INTEGER NOT NULL DEFAULT 0;
CREATE TABLE job_origins(job_id TEXT PRIMARY KEY REFERENCES jobs(id), origin TEXT NOT NULL);
CREATE TABLE origin_cooldowns(origin TEXT PRIMARY KEY, until_ms INTEGER NOT NULL);
PRAGMA user_version=3;
-- The caller fills job_origins and commits in the same transaction.
