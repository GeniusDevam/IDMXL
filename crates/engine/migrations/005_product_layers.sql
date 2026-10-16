BEGIN IMMEDIATE;
CREATE TABLE capture_handoffs (
    request_id TEXT PRIMARY KEY REFERENCES handoffs(request_id),
    phase TEXT NOT NULL,
    expires INTEGER NOT NULL,
    etag TEXT NOT NULL,
    total INTEGER NOT NULL
);
PRAGMA user_version=5;
CREATE TABLE media_jobs (job_id TEXT PRIMARY KEY REFERENCES jobs(id), plan BLOB NOT NULL);
CREATE TABLE media_components (job_id TEXT PRIMARY KEY REFERENCES jobs(id), parent_id TEXT NOT NULL REFERENCES jobs(id));
COMMIT;
