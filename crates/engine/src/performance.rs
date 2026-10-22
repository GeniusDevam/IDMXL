//! Explicit, numeric-only diagnostics. No URL, filename, cookie or header logging.
use anyhow::Result;
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::Path,
    time::Instant,
};

#[derive(Serialize)]
pub(crate) struct ResponseEvent {
    pub at_ms: i64,
    pub offset: Option<u64>,
    pub length: Option<u64>,
    pub http_status: u32,
    pub retry_after_ms: Option<u64>,
    pub invalid_retry_after: bool,
    pub attempt: u32,
    pub concurrency: usize,
}
#[derive(Serialize)]
pub(crate) struct RecoveryEvent {
    pub at_ms: i64,
    pub next_attempt_ms: i64,
    pub attempt: u32,
    pub http_status: u32,
    pub retry_after_ms: Option<u64>,
    pub invalid_retry_after: bool,
}
#[derive(Serialize)]
pub(crate) struct Metrics {
    #[serde(skip)]
    output: Option<File>,
    #[serde(skip)]
    started: Instant,
    pub elapsed_ms: f64,
    pub other_ms: f64,
    pub probe_ms: f64,
    pub resume_validation_ms: f64,
    pub download_ms: f64,
    pub final_flush_ms: f64,
    pub verification_tail_ms: f64,
    pub publication_ms: f64,
    pub checkpoint_flush_ms: f64,
    pub checkpoint_commit_ms: f64,
    pub request_accounting_ms: f64,
    pub callback_write_ms: f64,
    pub callback_hash_ms: f64,
    pub storage_write_ms: f64,
    pub storage_hash_ms: f64,
    pub storage_drain_ms: f64,
    pub storage_queue_peak_bytes: usize,
    pub storage_backpressure_events: u64,
    pub requests: u64,
    pub finished_requests: u64,
    pub new_connections: u64,
    pub checkpoint_batches: u64,
    pub checkpoint_blocks: u64,
    pub checkpoint_target_bytes: u64,
    pub checkpoint_interval_ms: u64,
    pub http_versions: BTreeMap<String, u64>,
    pub http_statuses: BTreeMap<u32, u64>,
    pub response_events: VecDeque<ResponseEvent>,
    pub recovery_schedules: VecDeque<RecoveryEvent>,
    pub request_dns_ms: f64,
    pub request_connect_ms: f64,
    pub request_tls_ms: f64,
    pub request_first_byte_ms: f64,
    pub status: String,
}

impl Metrics {
    pub fn new(path: Option<&Path>) -> Result<Self> {
        let output = path
            .map(|p| OpenOptions::new().write(true).create_new(true).open(p))
            .transpose()?;
        Ok(Self {
            output,
            started: Instant::now(),
            elapsed_ms: 0.,
            other_ms: 0.,
            probe_ms: 0.,
            resume_validation_ms: 0.,
            download_ms: 0.,
            final_flush_ms: 0.,
            verification_tail_ms: 0.,
            publication_ms: 0.,
            checkpoint_flush_ms: 0.,
            checkpoint_commit_ms: 0.,
            request_accounting_ms: 0.,
            callback_write_ms: 0.,
            callback_hash_ms: 0.,
            storage_write_ms: 0.,
            storage_hash_ms: 0.,
            storage_drain_ms: 0.,
            storage_queue_peak_bytes: 0,
            storage_backpressure_events: 0,
            requests: 0,
            finished_requests: 0,
            new_connections: 0,
            checkpoint_batches: 0,
            checkpoint_blocks: 0,
            checkpoint_target_bytes: 0,
            checkpoint_interval_ms: 0,
            http_versions: BTreeMap::new(),
            http_statuses: BTreeMap::new(),
            response_events: VecDeque::new(),
            recovery_schedules: VecDeque::new(),
            request_dns_ms: 0.,
            request_connect_ms: 0.,
            request_tls_ms: 0.,
            request_first_byte_ms: 0.,
            status: "incomplete".into(),
        })
    }
    pub fn enabled(&self) -> bool {
        self.output.is_some()
    }
    pub fn request(&mut self, easy: &curl::easy::Easy2<crate::transport::Receiver>) {
        self.finished_requests += 1;
        self.storage_backpressure_events += easy.get_ref().backpressure_events;
        if !self.enabled() {
            return;
        }
        self.new_connections += easy.num_connects().unwrap_or(0);
        *self
            .http_versions
            .entry(easy.get_ref().headers.protocol.clone())
            .or_default() += 1;
        self.request_dns_ms += easy.namelookup_time().unwrap_or_default().as_secs_f64() * 1000.;
        self.request_connect_ms += easy.connect_time().unwrap_or_default().as_secs_f64() * 1000.;
        self.request_tls_ms += easy.appconnect_time().unwrap_or_default().as_secs_f64() * 1000.;
        self.request_first_byte_ms +=
            easy.starttransfer_time().unwrap_or_default().as_secs_f64() * 1000.;
        self.callback_write_ms += easy.get_ref().write_ms;
        self.callback_hash_ms += easy.get_ref().hash_ms;
    }
    pub fn response(
        &mut self,
        receiver: &crate::transport::Receiver,
        attempt: u32,
        concurrency: usize,
    ) {
        *self
            .http_statuses
            .entry(receiver.headers.status)
            .or_default() += 1;
        if !self.enabled() {
            return;
        }
        let (offset, length) = match receiver.mode {
            crate::transport::Mode::Range { offset, length, .. } => (Some(offset), Some(length)),
            _ => (None, None),
        };
        let raw = receiver.headers.fields.get("retry-after");
        let delay = raw.and_then(|s| crate::engine::server_delay(s));
        if self.response_events.len() >= 256 {
            self.response_events.pop_front();
        }
        self.response_events.push_back(ResponseEvent {
            at_ms: crate::store::now(),
            offset,
            length,
            http_status: receiver.headers.status,
            retry_after_ms: delay.map(|d| d.as_millis().min(u64::MAX as u128) as u64),
            invalid_retry_after: raw.is_some() && delay.is_none(),
            attempt,
            concurrency,
        });
    }
    pub fn recovery(&mut self, deadline: i64, attempt: u32, event: &crate::transport::Throttled) {
        if self.recovery_schedules.len() >= 128 {
            self.recovery_schedules.pop_front();
        }
        self.recovery_schedules.push_back(RecoveryEvent {
            at_ms: crate::store::now(),
            next_attempt_ms: deadline,
            attempt,
            http_status: event.status,
            retry_after_ms: event
                .delay
                .map(|d| d.as_millis().min(u64::MAX as u128) as u64),
            invalid_retry_after: event.invalid_retry_after,
        });
    }
    pub fn snapshot(&mut self, status: &str) -> Result<()> {
        self.elapsed_ms = self.started.elapsed().as_secs_f64() * 1000.;
        self.other_ms = (self.elapsed_ms
            - self.probe_ms
            - self.resume_validation_ms
            - self.download_ms
            - self.final_flush_ms
            - self.verification_tail_ms
            - self.publication_ms)
            .max(0.);
        self.status = status.into();
        if let Some(mut output) = self.output.take() {
            let result = (|| -> Result<()> {
                output.seek(SeekFrom::Start(0))?;
                output.set_len(0)?;
                serde_json::to_writer_pretty(&mut output, &self)?;
                output.write_all(b"\n")?;
                output.flush()?;
                Ok(())
            })();
            self.output = Some(output);
            result?;
        }
        Ok(())
    }
    pub fn finish(&mut self, status: &str) -> Result<()> {
        let result = self.snapshot(status);
        self.output.take();
        result
    }
}

pub(crate) fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}
