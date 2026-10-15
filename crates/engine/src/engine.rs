use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{File, OpenOptions},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use curl::multi::{Easy2Handle, Multi};

use crate::{
    Store,
    model::{Extent, Job, JobView},
    platform,
    transport::{self, Mode, Receiver},
};

const BLOCK: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct RunOptions {
    pub connections: usize,
    pub retries: u32,
    pub speed_limit_bytes: u64,
    pub stop: Arc<AtomicBool>,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            connections: 2,
            retries: 2,
            speed_limit_bytes: 0,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub struct Engine {
    pub store: Store,
}

#[derive(Clone)]
struct Work {
    offset: u64,
    length: u64,
    attempt: u32,
    ready: Instant,
}

struct Meter {
    last: Instant,
    bytes: u64,
}
impl Meter {
    fn new(bytes: u64) -> Self {
        Self {
            last: Instant::now(),
            bytes,
        }
    }
    fn update(&mut self, store: &Store, id: &str, received: u64, total: Option<u64>) -> Result<()> {
        let elapsed = self.last.elapsed();
        if elapsed < Duration::from_millis(250) {
            return Ok(());
        }
        let speed = (received.saturating_sub(self.bytes) as f64 / elapsed.as_secs_f64()) as u64;
        store.progress(id, received, speed, total)?;
        self.last = Instant::now();
        self.bytes = received;
        Ok(())
    }
}

impl Engine {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Reconcile orphaned active states only while no transfer process owns the profile.
    pub fn reconcile(&self) -> Result<()> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.store.directory.join("engine.lock"))?;
        if fs2::FileExt::try_lock_exclusive(&lock).is_ok() {
            self.store.recover_active()?;
        }
        Ok(())
    }

    pub fn run(&self, id: &str, options: RunOptions) -> Result<JobView> {
        if !(1..=4).contains(&options.connections) || options.retries > 5 {
            bail!("invalid_transfer_options");
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.store.directory.join("engine.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock).context("engine_busy")?;
        self.store.recover_active()?;
        let job = self.store.get(id)?;
        if job.state == "completed" {
            return self.store.view(id);
        }
        if job.state == "canceled" {
            bail!("job_canceled");
        }
        if job.state == "publishing" {
            return self.recover_publication(&job);
        }
        if self.store.retry_deadline(&job.url)?.is_some() {
            bail!("server_retry_pending");
        }
        self.store.begin(id)?;
        match self.transfer(id, &options) {
            Ok(()) => self.store.view(id),
            Err(error) => {
                if let Some(throttle) = error.downcast_ref::<transport::Throttled>()
                    && let Some(delay) = throttle.delay
                {
                    self.store.defer_origin(&job.url, delay)?;
                    self.store.defer_origin(&throttle.url, delay)?;
                }
                let message = error.to_string();
                let safe = message.split(':').next().unwrap_or("transfer_failed");
                let known = [
                    "resource_changed",
                    "range_ignored",
                    "invalid_content_range",
                    "invalid_content_length",
                    "body_exceeds_lease",
                    "encoded_response_unsupported",
                    "http_request_failed",
                    "server_throttled",
                    "retry_exhausted",
                    "transfer_transport_failed",
                    "storage_write_failed",
                    "insufficient_disk_space",
                    "digest_mismatch",
                    "destination_exists",
                    "staging_missing",
                    "staging_reparse_point",
                    "staging_truncated",
                    "publication_failed",
                    "attachment_metadata_failed",
                    "headers_too_large",
                    "duplicate_identity_header",
                    "unexpected_response",
                    "unexpected_partial_response",
                    "insecure_redirect",
                    "too_many_redirects",
                    "tls_verification_failed",
                    "probe_transport_failed",
                    "invalid_probe_response",
                    "transfer_interrupted",
                    "private_network_blocked",
                    "dns_resolution_failed",
                ];
                let code = if known.contains(&safe) {
                    safe
                } else {
                    "transfer_failed"
                };
                let current = self.store.get(id)?;
                if current.state == "publishing" {
                    self.store.state(id, "publishing", Some(code))?;
                } else if options.stop.load(Ordering::Relaxed) || self.store.requested(id)? != "run"
                {
                    let state = if self.store.requested(id)? == "canceled" {
                        "canceled"
                    } else {
                        "paused"
                    };
                    self.store.state(id, state, None)?;
                    return self.store.view(id);
                } else {
                    self.store.state(id, "failed", Some(code))?;
                }
                bail!("{code}")
            }
        }
    }

    fn transfer(&self, id: &str, options: &RunOptions) -> Result<()> {
        let mut job = self.store.get(id)?;
        if job.destination.exists() {
            bail!("destination_exists");
        }
        let probe = if job.replayable {
            Some(transport::probe(
                &job.url,
                options.stop.clone(),
                job.public_only,
            )?)
        } else {
            None
        };
        let ranged = probe.as_ref().is_some_and(|p| p.ranged);
        let resolved = probe
            .as_ref()
            .map(|p| p.resolved.as_str())
            .unwrap_or(&job.url)
            .to_owned();
        let total = probe.as_ref().and_then(|p| p.total);
        let etag = probe.as_ref().and_then(|p| p.etag.as_deref());
        let reuse = ranged
            && job.mode == "ranged"
            && job.total == total
            && job.etag.as_deref() == etag
            && job.resolved_url.as_deref() == Some(&resolved)
            && job.staging.exists();
        self.store.identity(
            id,
            total,
            etag,
            &resolved,
            if ranged { "ranged" } else { "sequential" },
            !reuse,
        )?;
        job = self.store.get(id)?;
        let file = if reuse {
            if platform::is_reparse(&job.staging)? {
                bail!("staging_reparse_point");
            }
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&job.staging)?
        } else {
            if job.staging.exists() {
                if platform::is_reparse(&job.staging)? {
                    bail!("staging_reparse_point");
                }
                std::fs::remove_file(&job.staging)?;
            }
            OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&job.staging)?
        };
        if let Some(total) = total {
            let needed = total.saturating_sub(job.durable);
            if fs2::available_space(job.destination.parent().context("invalid_destination")?)?
                < needed
            {
                bail!("insufficient_disk_space");
            }
        }
        let file = Arc::new(file);
        if ranged {
            self.download_ranges(&job, file.clone(), options)?;
        } else {
            self.download_sequential(&job, &resolved, file.clone(), options)?;
        }
        if self.should_stop(id, options)? {
            file.sync_all()?;
            let state = if self.store.requested(id)? == "canceled" {
                "canceled"
            } else {
                "paused"
            };
            self.store.state(id, state, None)?;
            return Ok(());
        }
        self.store.state(id, "verifying", None)?;
        file.sync_all()?;
        let size = file.metadata()?.len();
        let digest = platform::hash_extent(&file, 0, size)?;
        if job
            .expected_sha256
            .as_ref()
            .is_some_and(|expected| *expected != digest)
        {
            bail!("digest_mismatch");
        }
        if self.should_stop(id, options)? {
            let state = if self.store.requested(id)? == "canceled" {
                "canceled"
            } else {
                "paused"
            };
            self.store.state(id, state, None)?;
            return Ok(());
        }
        let file_id = platform::file_id(&file)?;
        self.store.publication_intent(id, size, &digest, &file_id)?;
        crate::fault::point("after_publication_intent");
        platform::mark_origin(&job.staging)?;
        crate::fault::point("after_origin_metadata");
        drop(file);
        platform::publish(&job.staging, &job.destination)?;
        crate::fault::point("after_rename");
        crate::fault::point("before_completion_commit");
        self.store.complete(id)
    }

    fn download_ranges(&self, job: &Job, file: Arc<File>, options: &RunOptions) -> Result<()> {
        let total = job.total.context("missing_range_length")?;
        let completed = self.store.extents(&job.id, job.generation)?;
        let mut valid = HashSet::new();
        for extent in completed {
            let expected_length = BLOCK.min(total.saturating_sub(extent.offset));
            let sound = extent.offset.is_multiple_of(BLOCK)
                && extent.length == expected_length
                && extent.length > 0
                && platform::hash_extent(&file, extent.offset, extent.length)
                    .is_ok_and(|hash| hash == extent.digest);
            if sound {
                valid.insert(extent.offset);
            } else {
                self.store.forget_extent(&job.id, extent.offset)?;
            }
        }
        file.set_len(total)?;
        let mut durable = self.store.get(&job.id)?.durable;
        let mut meter = Meter::new(durable);
        let mut pending: VecDeque<Work> = VecDeque::new();
        let mut cursor = 0;
        let multi = Multi::new();
        let mut active: HashMap<usize, (Easy2Handle<Receiver>, Work)> = HashMap::new();
        let connections = if options.speed_limit_bytes > 0 {
            options
                .connections
                .min(options.speed_limit_bytes.min(4) as usize)
        } else {
            options.connections
        };
        let speed = if options.speed_limit_bytes > 0 {
            options.speed_limit_bytes / connections as u64
        } else {
            0
        };
        let mut token = 0usize;
        let mut concurrency = connections;
        let mut origin_retry_until = Instant::now();
        while cursor < total || !pending.is_empty() || !active.is_empty() {
            if self.should_stop(&job.id, options)? {
                return Ok(());
            }
            while active.len() < concurrency {
                if Instant::now() < origin_retry_until {
                    break;
                }
                let task = if let Some(index) =
                    pending.iter().position(|task| task.ready <= Instant::now())
                {
                    pending.remove(index).expect("known retry index")
                } else if !pending.is_empty() {
                    break;
                } else {
                    while cursor < total && valid.contains(&cursor) {
                        cursor += BLOCK.min(total - cursor);
                    }
                    if cursor == total {
                        break;
                    }
                    let task = Work {
                        offset: cursor,
                        length: BLOCK.min(total - cursor),
                        attempt: 0,
                        ready: Instant::now(),
                    };
                    cursor += task.length;
                    task
                };
                let mode = Mode::Range {
                    offset: task.offset,
                    length: task.length,
                    total,
                    etag: job.etag.clone().context("missing_range_validator")?,
                };
                let easy = transport::handle(
                    job.resolved_url
                        .as_deref()
                        .context("missing_resolved_url")?,
                    mode,
                    Some(file.clone()),
                    options.stop.clone(),
                    speed,
                    job.public_only,
                )?;
                token += 1;
                let mut handle = multi.add2(easy)?;
                handle.set_token(token)?;
                active.insert(token, (handle, task));
            }
            multi.perform()?;
            let in_flight: u64 = active
                .values()
                .map(|(handle, _)| handle.get_ref().bytes)
                .sum();
            meter.update(
                &self.store,
                &job.id,
                durable.saturating_add(in_flight),
                Some(total),
            )?;
            let mut results = Vec::new();
            multi.messages(|message| {
                if let Some(result) = message.result()
                    && let Ok(token) = message.token()
                {
                    results.push((token, result));
                }
            });
            for (token, result) in results {
                let (handle, mut task) = active.remove(&token).context("unknown_transfer_token")?;
                let easy = multi.remove2(handle)?;
                let receiver = easy.get_ref();
                let error = receiver
                    .error
                    .or_else(|| {
                        result.as_ref().err().map(|error| {
                            if matches!(error.code(), 35 | 51 | 58 | 60 | 77) {
                                "tls_verification_failed"
                            } else {
                                "transfer_transport_failed"
                            }
                        })
                    })
                    .or_else(|| {
                        (receiver.headers.status != 206 || receiver.bytes != task.length)
                            .then_some("invalid_content_length")
                    });
                if let Some(error) = error {
                    if error == "server_throttled"
                        && let Some(header) = receiver.headers.fields.get("retry-after")
                        && let Some(delay) = server_delay(header)
                    {
                        self.store
                            .defer_origin(job.resolved_url.as_deref().unwrap_or(&job.url), delay)?;
                        self.store.defer_origin(&job.url, delay)?;
                    }
                    let retryable = error == "server_throttled"
                        || (error == "transfer_transport_failed"
                            && result.as_ref().err().is_some_and(|e| {
                                matches!(e.code(), 5 | 6 | 7 | 18 | 28 | 52 | 55 | 56)
                            }));
                    if retryable && task.attempt < options.retries {
                        task.attempt += 1;
                        let Some(delay) = retry_delay(
                            receiver
                                .headers
                                .fields
                                .get("retry-after")
                                .map(String::as_str),
                            task.attempt,
                        ) else {
                            // A long or unparsed server deadline must not be retried early.
                            bail!("server_throttled");
                        };
                        task.ready = Instant::now() + delay;
                        if error == "server_throttled" {
                            concurrency = 1;
                            origin_retry_until = origin_retry_until.max(task.ready);
                        }
                        pending.push_back(task);
                        continue;
                    }
                    bail!("{error}");
                }
                // The file flush precedes every durable extent transaction.
                file.sync_all()?;
                crate::fault::point("after_extent_flush");
                self.store.checkpoint(
                    &job.id,
                    job.generation,
                    &Extent {
                        offset: task.offset,
                        length: task.length,
                        digest: receiver.digest(),
                    },
                )?;
                durable += task.length;
            }
            if active.is_empty() {
                std::thread::sleep(Duration::from_millis(25));
            } else {
                multi.wait(&mut [], Duration::from_millis(100))?;
            }
        }
        Ok(())
    }

    fn download_sequential(
        &self,
        job: &Job,
        url: &str,
        file: Arc<File>,
        options: &RunOptions,
    ) -> Result<()> {
        let mut resolved = url.to_owned();
        for _ in 0..=5 {
            let easy = transport::handle(
                &resolved,
                Mode::Sequential,
                Some(file.clone()),
                options.stop.clone(),
                options.speed_limit_bytes,
                job.public_only,
            )?;
            let multi = Multi::new();
            let handle = multi.add2(easy)?;
            let mut meter = Meter::new(0);
            loop {
                if self.should_stop(&job.id, options)? {
                    return Ok(());
                }
                let running = multi.perform()?;
                let receiver = handle.get_ref();
                if receiver.headers.status == 200 && receiver.error.is_none() {
                    meter.update(
                        &self.store,
                        &job.id,
                        receiver.bytes,
                        receiver.headers.length(),
                    )?;
                }
                if running == 0 {
                    break;
                }
                multi.wait(&mut [], Duration::from_millis(100))?;
            }
            let mut result = Ok(());
            multi.messages(|message| {
                if let Some(r) = message.result() {
                    result = r;
                }
            });
            let easy = multi.remove2(handle)?;
            let receiver = easy.get_ref();
            if let Some(error) = receiver.error {
                if error == "server_throttled"
                    && let Some(delay) = receiver
                        .headers
                        .fields
                        .get("retry-after")
                        .and_then(|n| server_delay(n))
                {
                    self.store.defer_origin(&job.url, delay)?;
                    self.store.defer_origin(&resolved, delay)?;
                }
                bail!("{error}");
            }
            let status = receiver.headers.status;
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let next = platform::checked_url(&resolved)?.join(
                    receiver
                        .headers
                        .fields
                        .get("location")
                        .context("redirect_without_location")?,
                )?;
                platform::checked_url(next.as_str())?;
                if platform::checked_url(&resolved)?.scheme() == "https" && next.scheme() != "https"
                {
                    bail!("insecure_redirect");
                }
                resolved = next.to_string();
                continue;
            }
            result.map_err(|error| anyhow::anyhow!(transport::transport_error(&error)))?;
            if status != 200
                || receiver
                    .headers
                    .length()
                    .is_some_and(|n| n != receiver.bytes)
            {
                bail!("invalid_content_length");
            }
            file.set_len(receiver.bytes)?;
            file.sync_all()?;
            self.store.checkpoint(
                &job.id,
                job.generation,
                &Extent {
                    offset: 0,
                    length: receiver.bytes,
                    digest: receiver.digest(),
                },
            )?;
            return Ok(());
        }
        bail!("too_many_redirects")
    }

    fn should_stop(&self, id: &str, options: &RunOptions) -> Result<bool> {
        Ok(options.stop.load(Ordering::Relaxed) || self.store.requested(id)? != "run")
    }

    fn recover_publication(&self, job: &Job) -> Result<JobView> {
        if job.destination.exists() {
            if platform::is_reparse(&job.destination)? {
                bail!("publication_identity_mismatch");
            }
            let file = File::open(&job.destination)?;
            let size = file.metadata()?.len();
            if job.publication_file_id.as_deref() != Some(platform::file_id(&file)?.as_str())
                || job.total != Some(size)
                || job.actual_sha256.as_deref()
                    != Some(platform::hash_extent(&file, 0, size)?.as_str())
            {
                bail!("publication_identity_mismatch");
            }
            self.store.complete(&job.id)?;
        } else if job.staging.exists() {
            if platform::is_reparse(&job.staging)? {
                bail!("staging_reparse_point");
            }
            let file = File::open(&job.staging)?;
            let size = file.metadata()?.len();
            if job.publication_file_id.as_deref() != Some(platform::file_id(&file)?.as_str())
                || job.total != Some(size)
                || job.actual_sha256.as_deref()
                    != Some(platform::hash_extent(&file, 0, size)?.as_str())
            {
                bail!("publication_identity_mismatch");
            }
            platform::mark_origin(&job.staging)?;
            drop(file);
            platform::publish(&job.staging, &job.destination)?;
            self.store.complete(&job.id)?;
        } else {
            bail!("publication_file_missing");
        }
        self.store.view(&job.id)
    }
}

fn retry_delay(header: Option<&str>, attempt: u32) -> Option<Duration> {
    if let Some(header) = header {
        let delay = server_delay(header)?;
        return (delay <= Duration::from_secs(300)).then_some(delay);
    }
    let ceiling = 200u128.checked_shl(attempt.min(5)).unwrap_or(6400);
    Some(Duration::from_millis(
        (uuid::Uuid::new_v4().as_u128() % ceiling) as u64,
    ))
}

pub(crate) fn server_delay(header: &str) -> Option<Duration> {
    if let Ok(seconds) = header.parse::<u64>() {
        Some(Duration::from_secs(seconds))
    } else {
        Some(
            httpdate::parse_http_date(header)
                .ok()?
                .duration_since(std::time::SystemTime::now())
                .unwrap_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AddJob;
    #[test]
    fn long_or_unparsed_retry_after_never_retries_early() {
        assert_eq!(retry_delay(Some("5"), 1), Some(Duration::from_secs(5)));
        assert!(retry_delay(Some("3600"), 1).is_none());
        assert!(retry_delay(Some("not a date"), 1).is_none());
        let soon = std::time::SystemTime::now() + Duration::from_secs(5);
        let delay = retry_delay(Some(&httpdate::fmt_http_date(soon)), 1).unwrap();
        assert!(delay > Duration::from_secs(3) && delay <= Duration::from_secs(5));
    }
    #[test]
    fn publication_recovers_after_rename_before_database_commit() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(&root.path().join("state")).unwrap();
        let job = store
            .add(AddJob {
                url: "http://example.test/file".into(),
                destination: root.path().join("result.bin"),
                replayable: false,
                expected_sha256: None,
            })
            .unwrap();
        let job = store.get(&job.id).unwrap();
        std::fs::write(&job.staging, b"complete").unwrap();
        let file = File::open(&job.staging).unwrap();
        let digest = platform::hash_extent(&file, 0, 8).unwrap();
        store
            .publication_intent(&job.id, 8, &digest, &platform::file_id(&file).unwrap())
            .unwrap();
        drop(file);
        platform::publish(&job.staging, &job.destination).unwrap();
        let result = Engine::new(store)
            .run(&job.id, RunOptions::default())
            .unwrap();
        assert_eq!(result.state, "completed");
    }
    #[test]
    fn publication_does_not_accept_an_unrelated_identical_file() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(&root.path().join("state")).unwrap();
        let view = store
            .add(AddJob {
                url: "http://example.test/file".into(),
                destination: root.path().join("result.bin"),
                replayable: false,
                expected_sha256: None,
            })
            .unwrap();
        let job = store.get(&view.id).unwrap();
        std::fs::write(&job.staging, b"complete").unwrap();
        let file = File::open(&job.staging).unwrap();
        store
            .publication_intent(
                &job.id,
                8,
                &platform::hash_extent(&file, 0, 8).unwrap(),
                &platform::file_id(&file).unwrap(),
            )
            .unwrap();
        drop(file);
        std::fs::write(&job.destination, b"complete").unwrap();
        assert!(
            Engine::new(store)
                .run(&job.id, RunOptions::default())
                .is_err()
        );
        assert!(job.staging.exists());
    }
}
