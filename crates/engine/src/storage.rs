//! Bounded network-to-disk queue. One worker owns range writes and durable batches.
use crate::{Store, model::Extent, platform, transport::RECOVERY_BLOCK};
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

const WRITE_BATCH: usize = 128 * 1024;
const QUEUE_BYTES: usize = 16 * 1024 * 1024;
const CHECKPOINT_BYTES: u64 = 8 * RECOVERY_BLOCK;
const CHECKPOINT_INTERVAL: Duration = Duration::from_millis(500);

struct CheckpointPolicy {
    bytes: u64,
    interval: Duration,
}
impl CheckpointPolicy {
    fn for_size(total: u64) -> Self {
        if total >= 32 * RECOVERY_BLOCK {
            Self {
                bytes: 32 * RECOVERY_BLOCK,
                interval: Duration::from_secs(2),
            }
        } else {
            Self {
                bytes: CHECKPOINT_BYTES,
                interval: CHECKPOINT_INTERVAL,
            }
        }
    }
}

enum Command {
    Write {
        token: usize,
        start: u64,
        length: u64,
        offset: u64,
        data: Vec<u8>,
    },
    End(usize),
    Finish,
}
pub(crate) enum Event {
    Written(Extent),
}

#[derive(Default)]
pub(crate) struct Stats {
    pub write_ms: f64,
    pub hash_ms: f64,
    pub flush_ms: f64,
    pub commit_ms: f64,
    pub batches: u64,
    pub blocks: u64,
    pub queue_peak_bytes: usize,
    pub checkpoint_target_bytes: u64,
    pub checkpoint_interval_ms: u64,
}

#[derive(Clone)]
pub(crate) struct Sender {
    tx: SyncSender<Command>,
    queued: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    limit: usize,
}
impl Sender {
    pub fn write(
        &self,
        token: usize,
        start: u64,
        length: u64,
        offset: u64,
        data: &[u8],
    ) -> Result<bool> {
        let before = self.queued.fetch_add(data.len(), Ordering::Relaxed);
        if before.saturating_add(data.len()) > self.limit {
            self.queued.fetch_sub(data.len(), Ordering::Relaxed);
            return Ok(false);
        }
        match self.tx.try_send(Command::Write {
            token,
            start,
            length,
            offset,
            data: data.to_vec(),
        }) {
            Ok(()) => {
                self.peak.fetch_max(before + data.len(), Ordering::Relaxed);
                Ok(true)
            }
            Err(error) => {
                self.queued.fetch_sub(data.len(), Ordering::Relaxed);
                match error {
                    TrySendError::Full(_) => Ok(false),
                    TrySendError::Disconnected(_) => bail!("storage_write_failed"),
                }
            }
        }
    }
    pub fn ready(&self) -> bool {
        self.queued.load(Ordering::Relaxed) < self.limit / 2
    }
}

pub(crate) struct Target {
    pub sender: Sender,
    pub token: usize,
    pub start: u64,
    pub length: u64,
}

pub(crate) struct Pipeline {
    sender: Sender,
    events: Receiver<Event>,
    worker: Option<JoinHandle<Result<Stats>>>,
}
impl Pipeline {
    pub fn new(
        store: &Store,
        id: &str,
        generation: u64,
        file: Arc<File>,
        instrument: bool,
        total: u64,
    ) -> Result<Self> {
        let writer = Store::open(&store.directory)?;
        let limit = QUEUE_BYTES;
        #[cfg(feature = "fault-injection")]
        let limit = std::env::var("IDMXL_TEST_STORAGE_QUEUE_BYTES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|n| n.clamp(WRITE_BATCH, QUEUE_BYTES))
            .unwrap_or(limit);
        let (tx, rx) = mpsc::sync_channel((limit / 16384).max(1));
        let (events, event_rx) = mpsc::channel();
        let queued = Arc::new(AtomicUsize::new(0));
        let shared = queued.clone();
        let peak = Arc::new(AtomicUsize::new(0));
        let id = id.to_owned();
        let worker = std::thread::spawn(move || {
            run(
                writer,
                &id,
                generation,
                file,
                rx,
                events,
                shared,
                instrument,
                CheckpointPolicy::for_size(total),
            )
        });
        Ok(Self {
            sender: Sender {
                tx,
                queued,
                peak,
                limit,
            },
            events: event_rx,
            worker: Some(worker),
        })
    }
    pub fn target(&self, token: usize, start: u64, length: u64) -> Target {
        Target {
            sender: self.sender.clone(),
            token,
            start,
            length,
        }
    }
    pub fn end(&self, token: usize) -> Result<bool> {
        match self.sender.tx.try_send(Command::End(token)) {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => bail!("storage_write_failed"),
        }
    }
    pub fn events(&self) -> impl Iterator<Item = Event> + '_ {
        self.events.try_iter()
    }
    pub fn ready(&self) -> bool {
        self.sender.ready()
    }
    pub fn finish(&mut self) -> Result<Stats> {
        let sent = self.sender.tx.send(Command::Finish);
        let worker = self.worker.take().context("storage_worker_failed")?;
        let result = worker
            .join()
            .map_err(|_| anyhow::anyhow!("storage_worker_failed"))?;
        result.and_then(|mut stats| {
            sent.map_err(|_| anyhow::anyhow!("storage_worker_failed"))?;
            stats.queue_peak_bytes = self.sender.peak.load(Ordering::Relaxed);
            Ok(stats)
        })
    }
}
impl Drop for Pipeline {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.finish();
        }
    }
}

struct Lease {
    start: u64,
    length: u64,
    received: u64,
    buffer: Vec<u8>,
    hash: Sha256,
}
fn checkpoint(
    store: &Store,
    id: &str,
    generation: u64,
    file: &File,
    pending: &mut Vec<Extent>,
    stats: &mut Stats,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let phase = Instant::now();
    file.sync_all().context("storage_write_failed")?;
    stats.flush_ms += crate::performance::milliseconds(phase);
    crate::fault::point("after_extent_flush");
    let phase = Instant::now();
    store.checkpoint_many(id, generation, pending)?;
    stats.commit_ms += crate::performance::milliseconds(phase);
    stats.batches += 1;
    stats.blocks += pending.len() as u64;
    pending.clear();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run(
    store: Store,
    id: &str,
    generation: u64,
    file: Arc<File>,
    rx: Receiver<Command>,
    events: mpsc::Sender<Event>,
    queued: Arc<AtomicUsize>,
    instrument: bool,
    policy: CheckpointPolicy,
) -> Result<Stats> {
    let mut leases: HashMap<usize, Lease> = HashMap::new();
    let mut pending = Vec::new();
    let mut pending_bytes = 0;
    let mut last = Instant::now();
    let mut stats = Stats {
        checkpoint_target_bytes: policy.bytes,
        checkpoint_interval_ms: policy.interval.as_millis() as u64,
        ..Stats::default()
    };
    loop {
        let message = match rx.recv_timeout(policy.interval.saturating_sub(last.elapsed())) {
            Ok(message) => Some(message),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => Some(Command::Finish),
        };
        match message {
            Some(Command::Finish) => {
                checkpoint(&store, id, generation, &file, &mut pending, &mut stats)?;
                return Ok(stats);
            }
            Some(Command::End(token)) => {
                leases.remove(&token);
            }
            Some(Command::Write {
                token,
                start,
                length,
                offset,
                data,
            }) => {
                queued.fetch_sub(data.len(), Ordering::Relaxed);
                #[cfg(feature = "fault-injection")]
                if let Some(delay) = std::env::var("IDMXL_TEST_STORAGE_DELAY_MS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    std::thread::sleep(Duration::from_millis(delay.min(1000)));
                }
                let lease = leases.entry(token).or_insert_with(|| Lease {
                    start,
                    length,
                    received: 0,
                    buffer: Vec::with_capacity(WRITE_BATCH),
                    hash: Sha256::new(),
                });
                if lease.start != start
                    || lease.length != length
                    || offset != start + lease.received
                {
                    bail!("storage_write_order");
                }
                if data.len() as u64 > length.saturating_sub(lease.received) {
                    bail!("storage_write_order");
                }
                let mut remaining = data.as_slice();
                while !remaining.is_empty() {
                    let count = remaining
                        .len()
                        .min(WRITE_BATCH - lease.buffer.len())
                        .min((RECOVERY_BLOCK - lease.received % RECOVERY_BLOCK) as usize);
                    lease.buffer.extend_from_slice(&remaining[..count]);
                    lease.received += count as u64;
                    remaining = &remaining[count..];
                    let boundary = lease.received.is_multiple_of(RECOVERY_BLOCK)
                        || lease.received == lease.length;
                    if lease.buffer.len() == WRITE_BATCH || boundary {
                        let phase = instrument.then(Instant::now);
                        platform::write_at(
                            &file,
                            &lease.buffer,
                            start + lease.received - lease.buffer.len() as u64,
                        )
                        .context("storage_write_failed")?;
                        if let Some(phase) = phase {
                            stats.write_ms += crate::performance::milliseconds(phase);
                        }
                        crate::fault::point("after_body_write");
                        let phase = instrument.then(Instant::now);
                        lease.hash.update(&lease.buffer);
                        if let Some(phase) = phase {
                            stats.hash_ms += crate::performance::milliseconds(phase);
                        }
                        lease.buffer.clear();
                    }
                    if boundary {
                        let length = if lease.received.is_multiple_of(RECOVERY_BLOCK) {
                            RECOVERY_BLOCK
                        } else {
                            lease.received % RECOVERY_BLOCK
                        };
                        let offset = start + lease.received - length;
                        let extent = Extent {
                            offset,
                            length,
                            digest: format!("{:x}", lease.hash.finalize_reset()),
                        };
                        pending.push(extent.clone());
                        pending_bytes += length;
                        let _ = events.send(Event::Written(extent));
                    }
                }
            }
            None => {}
        }
        if pending_bytes >= policy.bytes || last.elapsed() >= policy.interval {
            checkpoint(&store, id, generation, &file, &mut pending, &mut stats)?;
            pending_bytes = 0;
            last = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_worker_write_never_commits_an_extent() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(&root.path().join("state")).unwrap();
        let view = store
            .add(crate::AddJob {
                url: "https://example.org/file".into(),
                destination: root.path().join("out.bin"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        let job = store.get(&view.id).unwrap();
        std::fs::write(&job.staging, b"original").unwrap();
        let mut pipeline = Pipeline::new(
            &store,
            &job.id,
            job.generation,
            Arc::new(File::open(&job.staging).unwrap()),
            true,
            3,
        )
        .unwrap();
        assert!(
            pipeline
                .target(1, 0, 3)
                .sender
                .write(1, 0, 3, 0, b"new")
                .unwrap()
        );
        assert!(pipeline.finish().is_err());
        assert!(store.extents(&job.id, job.generation).unwrap().is_empty());
        assert_eq!(store.get(&job.id).unwrap().durable, 0);
        assert_eq!(std::fs::read(&job.staging).unwrap(), b"original");
    }
}
