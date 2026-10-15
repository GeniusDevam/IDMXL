use crate::{Engine, RunOptions, Store};
use anyhow::{Context, Result};
use std::{
    fs::OpenOptions,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// One active job per profile. This makes the global rate ceiling cover every queue.
/// Failed and paused jobs require an explicit retry; one-use URLs are never replayed here.
pub fn drain(engine: &Engine, watch: bool, stop: Arc<AtomicBool>) -> Result<()> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(engine.store.directory.join("scheduler.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock).context("scheduler_busy")?;
    engine.reconcile()?;
    while !stop.load(Ordering::Relaxed) && engine.store.queue_running()? {
        let Some(id) = engine.store.next_queued()? else {
            if !watch {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
            continue;
        };
        let policy = engine.store.policy(&id)?;
        let global = engine.store.global_speed_limit()?;
        let speed = match (global, policy.speed_limit) {
            (0, n) | (n, 0) => n,
            (a, b) => a.min(b),
        };
        let job_stop = Arc::new(AtomicBool::new(false));
        let replayable = engine.store.get(&id)?.replayable;
        let baseline = engine.store.get(&id)?.durable;
        let watch_id = id.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let directory = engine.store.directory.clone();
        let signal = job_stop.clone();
        let done = finished.clone();
        let external = stop.clone();
        let quantum = Duration::from_secs(30);
        #[cfg(feature = "fault-injection")]
        let quantum = std::env::var("IDMXL_TEST_QUANTUM_MS")
            .ok()
            .and_then(|n| n.parse::<u64>().ok())
            .map(|n| Duration::from_millis(n.clamp(100, 30000)))
            .unwrap_or(quantum);
        let watcher = std::thread::spawn(move || {
            let Ok(store) = Store::open(&directory) else {
                signal.store(true, Ordering::Relaxed);
                return;
            };
            let started = std::time::Instant::now();
            while !done.load(Ordering::Relaxed) {
                if external.load(Ordering::Relaxed)
                    || !store.queue_running().unwrap_or(false)
                    || policy.deadline.is_some_and(|n| crate::store::now() >= n)
                {
                    signal.store(true, Ordering::Relaxed);
                    break;
                }
                if replayable
                    && started.elapsed() >= quantum
                    && store
                        .get(&watch_id)
                        .is_ok_and(|j| j.mode == "ranged" && j.durable > baseline)
                {
                    signal.store(true, Ordering::Relaxed);
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let result = engine.run(
            &id,
            RunOptions {
                connections: policy.connections as usize,
                retries: policy.retries as u32,
                speed_limit_bytes: speed,
                adaptive: false,
                stop: job_stop,
            },
        );
        finished.store(true, Ordering::Relaxed);
        let _ = watcher.join();
        if engine.store.get(&id)?.state == "paused" && engine.store.get(&id)?.requested == "run" {
            engine.store.request(&id, "queued")?;
        }
        if result
            .as_ref()
            .is_err_and(|e| e.to_string() == "engine_busy")
        {
            std::thread::sleep(Duration::from_millis(250));
        }
        // Transfer errors are recorded on their job; later queued jobs can still proceed.
        // An unexpected failure before begin() must not cause an unbounded retry loop.
        else if result.is_err()
            && matches!(
                engine.store.get(&id)?.state.as_str(),
                "queued" | "publishing"
            )
        {
            return result.map(|_| ());
        }
    }
    Ok(())
}
