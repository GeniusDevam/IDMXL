//! Everyday download controls. Tokens stay in the protected source; request history is numeric.
use crate::{JobView, Store, platform, transport};
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use std::fs::OpenOptions;

impl Store {
    pub(crate) fn setting_bool(&self, key: &str, fallback: bool) -> Result<bool> {
        let value: Option<String> = self
            .db
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        match value {
            None => Ok(fallback),
            Some(s) => s.parse().context("invalid_saved_setting"),
        }
    }
    pub fn set_completion_notifications(&self, enabled: bool) -> Result<()> {
        self.db.execute("INSERT INTO settings VALUES('notify_completed',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[enabled.to_string()])?;
        Ok(())
    }
    pub fn completion_notifications(&self) -> Result<bool> {
        self.setting_bool("notify_completed", false)
    }
    pub fn pending_completions(&self) -> Result<Vec<JobView>> {
        let mut q = self.db.prepare(
            "SELECT job_id FROM completion_events WHERE delivered=0 ORDER BY created LIMIT 32",
        )?;
        let ids = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().map(|id| self.view(id)).collect()
    }
    pub fn acknowledge_completion(&self, id: &str) -> Result<()> {
        self.db.execute(
            "UPDATE completion_events SET delivered=1 WHERE job_id=?1",
            [id],
        )?;
        Ok(())
    }
    pub fn reset_retry_budget(&self, id: &str) -> Result<()> {
        self.get(id)?;
        self.db.execute("UPDATE request_attempts SET budget_failures=0 WHERE job_id=?1 OR job_id IN(SELECT job_id FROM media_components WHERE parent_id=?1)",[id])?;
        Ok(())
    }
    pub(crate) fn request_counts(&self, id: &str) -> Result<(String, String)> {
        let (started,failed):(u64,u64)=self.db.query_row("SELECT COALESCE(SUM(started),0),COALESCE(SUM(total_failures),0) FROM request_attempts WHERE job_id=?1 OR job_id IN(SELECT job_id FROM media_components WHERE parent_id=?1)",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        Ok((started.to_string(), failed.to_string()))
    }
    pub(crate) fn start_request(
        &self,
        id: &str,
        generation: u64,
        offset: u64,
        length: i64,
        retries: u32,
    ) -> Result<()> {
        let tx = self.write_transaction()?;
        let used:u32=tx.query_row("SELECT budget_failures FROM request_attempts WHERE job_id=?1 AND generation=?2 AND offset=?3 AND length=?4",params![id,generation,offset,length],|r|r.get(0)).optional()?.unwrap_or(0);
        if used > retries {
            bail!("retry_exhausted");
        }
        tx.execute("INSERT INTO request_attempts(job_id,generation,offset,length,started) VALUES(?1,?2,?3,?4,1) ON CONFLICT(job_id,generation,offset,length) DO UPDATE SET started=started+1",params![id,generation,offset,length])?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn failed_request(
        &self,
        id: &str,
        generation: u64,
        offset: u64,
        length: i64,
    ) -> Result<u32> {
        let tx = self.write_transaction()?;
        tx.execute("UPDATE request_attempts SET budget_failures=budget_failures+1,total_failures=total_failures+1 WHERE job_id=?1 AND generation=?2 AND offset=?3 AND length=?4",params![id,generation,offset,length])?;
        let used=tx.query_row("SELECT budget_failures FROM request_attempts WHERE job_id=?1 AND generation=?2 AND offset=?3 AND length=?4",params![id,generation,offset,length],|r|r.get(0))?;
        tx.commit()?;
        crate::fault::point("after_retry_record");
        Ok(used)
    }

    /// Retry budgets follow recovery blocks, not the current grouping into HTTP
    /// requests. Regrouping holes after a crash must never replenish that budget.
    pub(crate) fn start_range(
        &self,
        id: &str,
        generation: u64,
        offset: u64,
        length: u64,
        retries: u32,
    ) -> Result<()> {
        let tx = self.write_transaction()?;
        let end = offset
            .checked_add(length)
            .context("invalid_content_range")?;
        let exhausted: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM request_attempts WHERE job_id=?1 AND generation=?2 AND offset>=?3 AND offset<?4 AND budget_failures>?5)",params![id,generation,offset,end,retries],|r|r.get(0))?;
        if exhausted {
            bail!("retry_exhausted");
        }
        // One start per actual HTTP request, using its first canonical block.
        let first = length.min(crate::transport::RECOVERY_BLOCK);
        tx.execute("INSERT INTO request_attempts(job_id,generation,offset,length,started) VALUES(?1,?2,?3,?4,1) ON CONFLICT(job_id,generation,offset,length) DO UPDATE SET started=started+1",params![id,generation,offset,first])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn failed_range(
        &self,
        id: &str,
        generation: u64,
        offset: u64,
        length: u64,
    ) -> Result<u32> {
        let tx = self.write_transaction()?;
        let mut cursor = offset;
        let end = offset
            .checked_add(length)
            .context("invalid_content_range")?;
        let mut used = 0;
        while cursor < end {
            let size = (end - cursor).min(crate::transport::RECOVERY_BLOCK);
            tx.execute("INSERT INTO request_attempts(job_id,generation,offset,length,budget_failures,total_failures) VALUES(?1,?2,?3,?4,1,?5) ON CONFLICT(job_id,generation,offset,length) DO UPDATE SET budget_failures=budget_failures+1,total_failures=total_failures+?5",params![id,generation,cursor,size,u8::from(cursor==offset)])?;
            let n: u32 = tx.query_row("SELECT budget_failures FROM request_attempts WHERE job_id=?1 AND generation=?2 AND offset=?3 AND length=?4",params![id,generation,cursor,size],|r|r.get(0))?;
            used = used.max(n);
            cursor += size;
        }
        tx.commit()?;
        crate::fault::point("after_retry_record");
        Ok(used)
    }
    /// Resume bytes only when a renewed URL names the same origin/path and strong representation.
    pub fn refresh_source(&self, id: &str, source: &str) -> Result<JobView> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join("engine.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock).context("engine_busy")?;
        let job = self.get(id)?;
        if !matches!(job.state.as_str(), "failed" | "paused" | "queued") || self.is_media(id)? {
            bail!("source_refresh_unsupported");
        }
        let captured:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM capture_handoffs JOIN handoffs USING(request_id) WHERE job_id=?1)",[id],|r|r.get(0))?;
        if captured {
            bail!("source_refresh_unsupported");
        }
        let new = platform::checked_url(source)?;
        let old = platform::checked_url(&job.url)?;
        if old.scheme() == "https" && new.scheme() != "https" {
            bail!("insecure_redirect");
        }
        let resolved = if job.durable > 0 {
            if !job.replayable
                || job.expected_sha256.is_none()
                || job.mode != "ranged"
                || new.origin() != old.origin()
                || new.path() != old.path()
            {
                bail!("source_identity_unproven");
            }
            let probe = transport::probe(
                new.as_str(),
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                job.public_only,
            )?;
            if probe.total != job.total
                || probe.etag != job.etag
                || probe.etag.is_none()
                || !probe.ranged
            {
                bail!("resource_changed");
            }
            Some(probe.resolved)
        } else {
            None
        };
        let tx = self.write_transaction()?;
        tx.execute("UPDATE jobs SET source=?2,display_url=?3,resolved_url=?4,state='paused',requested='paused',error=NULL WHERE id=?1",params![id,platform::protect(new.as_str().as_bytes())?,platform::display_url(&new),resolved.map(|s|platform::protect(s.as_bytes())).transpose()?])?;
        tx.execute(
            "UPDATE job_origins SET origin=?2 WHERE job_id=?1",
            params![id, new.origin().ascii_serialization()],
        )?;
        tx.execute(
            "UPDATE request_attempts SET budget_failures=0 WHERE job_id=?1",
            [id],
        )?;
        tx.commit()?;
        self.view(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AddJob;
    #[test]
    fn request_admission_waits_for_the_writer_and_reads_the_committed_retry_budget() {
        use std::{sync::mpsc, time::Duration};
        let root = tempfile::tempdir().unwrap();
        let writer = Store::open(root.path()).unwrap();
        let job = writer
            .add(AddJob {
                url: "https://example.org/file".into(),
                destination: root.path().join("file.bin"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        writer.start_request(&job.id, 0, 0, 1024, 2).unwrap();
        let reader = Store::open(root.path()).unwrap();
        writer.db.execute_batch("BEGIN IMMEDIATE").unwrap();
        writer
            .db
            .execute(
                "UPDATE request_attempts SET budget_failures=3 WHERE job_id=?1",
                [&job.id],
            )
            .unwrap();
        let (send, receive) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            send.send(
                reader
                    .start_request(&job.id, 0, 0, 1024, 2)
                    .map_err(|e| e.to_string()),
            )
            .unwrap();
        });
        let early = receive.recv_timeout(Duration::from_millis(200));
        writer.db.execute_batch("COMMIT").unwrap();
        worker.join().unwrap();
        assert!(
            matches!(early, Err(mpsc::RecvTimeoutError::Timeout)),
            "writer contention must wait rather than fail a live transfer: {early:?}"
        );
        assert_eq!(receive.recv().unwrap().unwrap_err(), "retry_exhausted");
    }
    #[test]
    fn retries_survive_reopen_and_reset_only_the_budget() {
        let root = tempfile::tempdir().unwrap();
        let s = Store::open(root.path()).unwrap();
        let j = s
            .add(AddJob {
                url: "https://example.org/a".into(),
                destination: root.path().join("a.bin"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        assert!(j.policy.adaptive);
        assert_eq!(j.policy.connections, 4);
        for _ in 0..3 {
            s.start_request(&j.id, 0, 0, 1024, 2).unwrap();
            s.failed_request(&j.id, 0, 0, 1024).unwrap();
        }
        drop(s);
        let s = Store::open(root.path()).unwrap();
        assert!(s.start_request(&j.id, 0, 0, 1024, 2).is_err());
        assert_eq!(s.request_counts(&j.id).unwrap(), ("3".into(), "3".into()));
        s.reset_retry_budget(&j.id).unwrap();
        s.start_request(&j.id, 0, 0, 1024, 2).unwrap();
        assert_eq!(s.request_counts(&j.id).unwrap(), ("4".into(), "3".into()));
        s.start_request(&j.id, 1, 0, 1024, 2).unwrap();
    }
    #[test]
    fn completion_events_are_durable_and_not_duplicated() {
        let root = tempfile::tempdir().unwrap();
        let s = Store::open(root.path()).unwrap();
        let j = s
            .add(AddJob {
                url: "https://example.org/a".into(),
                destination: root.path().join("a.bin"),
                replayable: false,
                expected_sha256: None,
            })
            .unwrap();
        s.db.execute("UPDATE jobs SET total=0 WHERE id=?1", [&j.id])
            .unwrap();
        s.complete(&j.id).unwrap();
        s.complete(&j.id).unwrap();
        drop(s);
        let s = Store::open(root.path()).unwrap();
        assert_eq!(s.pending_completions().unwrap().len(), 1);
        s.acknowledge_completion(&j.id).unwrap();
        assert!(s.pending_completions().unwrap().is_empty());
    }
    #[test]
    fn v5_profile_is_backed_up_before_everyday_migration() {
        let root = tempfile::tempdir().unwrap();
        let s = Store::open(root.path()).unwrap();
        let j = s
            .add(AddJob {
                url: "https://example.org/a".into(),
                destination: root.path().join("a.bin"),
                replayable: false,
                expected_sha256: None,
            })
            .unwrap();
        s.db.execute_batch(
            "DROP TABLE request_attempts; DROP TABLE completion_events; PRAGMA user_version=5;",
        )
        .unwrap();
        drop(s);
        let s = Store::open(root.path()).unwrap();
        assert_eq!(s.get(&j.id).unwrap().state, "queued");
        assert!(!s.completion_notifications().unwrap());
        let backup = std::fs::read_dir(root.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("jobs-v5-")
            })
            .unwrap();
        let old = rusqlite::Connection::open(backup).unwrap();
        assert_eq!(
            old.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            5
        );
        assert_eq!(
            old.query_row("SELECT id FROM jobs", [], |r| r.get::<_, String>(0))
                .unwrap(),
            j.id
        );
    }
}
