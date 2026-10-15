use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone)]
pub struct AddJob {
    pub url: String,
    pub destination: PathBuf,
    pub replayable: bool,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub url: String,
    pub display_url: String,
    pub destination: PathBuf,
    pub staging: PathBuf,
    pub replayable: bool,
    pub state: String,
    pub generation: u64,
    pub total: Option<u64>,
    pub etag: Option<String>,
    pub resolved_url: Option<String>,
    pub mode: String,
    pub durable: u64,
    pub expected_sha256: Option<String>,
    pub actual_sha256: Option<String>,
    pub error: Option<String>,
    pub requested: String,
    pub publication_file_id: Option<String>,
}

/// Decimal byte strings keep the UI/IPC contract precise beyond JavaScript's integer range.
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub id: String,
    pub source: String,
    pub destination: String,
    pub name: String,
    pub state: String,
    pub generation: u64,
    pub total_bytes: Option<String>,
    pub durable_bytes: String,
    pub mode: String,
    pub replayable: bool,
    pub sha256: Option<String>,
    pub error: Option<String>,
    pub file_available: bool,
}

impl From<Job> for JobView {
    fn from(job: Job) -> Self {
        Self {
            name: job
                .destination
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            file_available: job.state == "completed" && job.destination.is_file(),
            destination: job.destination.to_string_lossy().into_owned(),
            id: job.id,
            source: job.display_url,
            state: job.state,
            generation: job.generation,
            total_bytes: job.total.map(|n| n.to_string()),
            durable_bytes: job.durable.to_string(),
            mode: job.mode,
            replayable: job.replayable,
            sha256: job.actual_sha256,
            error: job.error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Extent {
    pub offset: u64,
    pub length: u64,
    pub digest: String,
}
