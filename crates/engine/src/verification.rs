//! Hash a growing immutable prefix using an independently opened read handle.
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

#[derive(Default)]
struct State {
    available: u64,
    stop: bool,
    finish: bool,
}

pub(crate) struct Verifier {
    state: Arc<(Mutex<State>, Condvar)>,
    worker: Option<JoinHandle<Result<(String, u64)>>>,
}
impl Verifier {
    pub fn new(mut file: File) -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let shared = state.clone();
        let worker = std::thread::spawn(move || {
            let (lock, signal) = &*shared;
            let mut hash = Sha256::new();
            let mut position = 0u64;
            let mut buffer = vec![0u8; 256 * 1024];
            loop {
                let mut state = lock
                    .lock()
                    .map_err(|_| anyhow::anyhow!("verification_worker_failed"))?;
                while !state.stop && !state.finish && position == state.available {
                    state = signal
                        .wait(state)
                        .map_err(|_| anyhow::anyhow!("verification_worker_failed"))?;
                }
                let available = state.available;
                let stop = state.stop;
                let finished = state.finish && position == available;
                drop(state);
                if stop || finished {
                    return Ok((format!("{:x}", hash.finalize()), position));
                }
                while position < available {
                    if lock
                        .lock()
                        .map_err(|_| anyhow::anyhow!("verification_worker_failed"))?
                        .stop
                    {
                        return Ok((format!("{:x}", hash.finalize()), position));
                    }
                    let count = (available - position).min(buffer.len() as u64) as usize;
                    file.read_exact(&mut buffer[..count])
                        .context("staging_truncated")?;
                    hash.update(&buffer[..count]);
                    position += count as u64;
                }
            }
        });
        Self {
            state,
            worker: Some(worker),
        }
    }
    pub fn advance(&self, prefix: u64) {
        let (lock, signal) = &*self.state;
        if let Ok(mut state) = lock.lock()
            && prefix > state.available
        {
            state.available = prefix;
            signal.notify_one();
        }
    }
    pub fn finish(mut self, total: u64) -> Result<String> {
        // Wait until the entire announced prefix has actually been read.
        let (lock, signal) = &*self.state;
        {
            let mut state = lock
                .lock()
                .map_err(|_| anyhow::anyhow!("verification_worker_failed"))?;
            state.available = total;
            state.finish = true;
        }
        signal.notify_one();
        let worker = self.worker.take().context("verification_worker_failed")?;
        let (hash, position) = worker
            .join()
            .map_err(|_| anyhow::anyhow!("verification_worker_failed"))??;
        if position != total {
            bail!("verification_incomplete");
        }
        Ok(hash)
    }
}
impl Drop for Verifier {
    fn drop(&mut self) {
        let (lock, signal) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            state.stop = true;
            signal.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};
    #[test]
    fn verifies_ordered_disk_bytes_after_out_of_order_writes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("data");
        let mut writer = File::create(&path).unwrap();
        writer.set_len(524289).unwrap();
        let verifier = Verifier::new(File::open(&path).unwrap());
        let first = vec![3; 262144];
        let last = vec![7; 262145];
        writer.seek(SeekFrom::Start(262144)).unwrap();
        writer.write_all(&last).unwrap();
        verifier.advance(0);
        writer.seek(SeekFrom::Start(0)).unwrap();
        writer.write_all(&first).unwrap();
        verifier.advance(262144);
        let mut expected = Sha256::new();
        expected.update(&first);
        expected.update(&last);
        assert_eq!(
            verifier.finish(524289).unwrap(),
            format!("{:x}", expected.finalize())
        );
    }
    #[test]
    fn truncated_disk_data_cannot_produce_a_verified_result() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("data");
        std::fs::write(&path, b"short").unwrap();
        let verifier = Verifier::new(File::open(path).unwrap());
        assert!(verifier.finish(100).is_err());
    }
}
