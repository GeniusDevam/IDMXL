use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddJob {
    pub url: String,
    pub destination: PathBuf,
    #[serde(default)]
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
    pub received: u64,
    pub speed: u64,
    pub eta: Option<u64>,
    pub public_only: bool,
    pub policy: JobPolicy,
    pub retry_not_before: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JobPolicy {
    pub queue_name: String,
    pub priority: i32,
    /// Unix milliseconds, UTC. Zero means immediately eligible.
    pub not_before: i64,
    pub deadline: Option<i64>,
    pub connections: u8,
    pub retries: u8,
    pub speed_limit: u64,
}

impl Default for JobPolicy {
    fn default() -> Self {
        Self {
            queue_name: "Default".into(),
            priority: 0,
            not_before: 0,
            deadline: None,
            connections: 2,
            retries: 2,
            speed_limit: 0,
        }
    }
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
    pub received_bytes: String,
    pub speed_bytes_per_second: String,
    pub eta_seconds: Option<String>,
    pub policy: JobPolicy,
    pub retry_not_before: Option<String>,
    pub mode: String,
    pub replayable: bool,
    pub sha256: Option<String>,
    pub error: Option<String>,
    pub file_available: bool,
    pub queue_window_open: bool,
}

impl From<Job> for JobView {
    fn from(job: Job) -> Self {
        Self {
            queue_window_open: true,
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
            received_bytes: job.received.max(job.durable).to_string(),
            speed_bytes_per_second: job.speed.to_string(),
            eta_seconds: job.eta.map(|n| n.to_string()),
            policy: job.policy,
            retry_not_before: job.retry_not_before.map(|n| n.to_string()),
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
