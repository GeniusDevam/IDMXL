//! Durable recovery deadlines and request admission shared by jobs on one origin.
use crate::{Store, platform, store::now, transport::Throttled};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use std::time::Duration;

const NORMAL_INTERVAL_MS: i64 = 50;

pub(crate) fn backoff(attempt: u32, server: Option<Duration>) -> Duration {
    let seconds = 5u64
        .saturating_mul(1u64 << attempt.saturating_sub(1).min(6))
        .min(300);
    let jitter = (uuid::Uuid::new_v4().as_u128() % 1001) as u64;
    let fallback = Duration::from_secs(seconds) + Duration::from_millis(jitter);
    fallback.max(server.unwrap_or_default())
}

impl Store {
    pub(crate) fn expire_recoveries(&self) -> Result<()> {
        self.db.execute("UPDATE jobs SET state='failed',error='recovery_timed_out',speed=0,eta=NULL WHERE state='waiting' AND requested='run' AND EXISTS(SELECT 1 FROM recovery_jobs r JOIN policies p ON p.job_id=r.job_id WHERE r.job_id=jobs.id AND r.generation=jobs.generation AND r.durable>=jobs.durable AND r.since_ms+p.recovery_timeout_seconds*1000<=?1)",[now()])?;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn recovery_expired(&self, id: &str, timeout: Duration) -> Result<bool> {
        let expired:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM recovery_jobs r JOIN jobs j ON j.id=r.job_id WHERE r.job_id=?1 AND r.generation=j.generation AND r.durable>=j.durable AND r.since_ms+?2<=?3)",params![id,timeout.as_millis().min(i64::MAX as u128) as i64,now()],|r|r.get(0))?;
        Ok(expired)
    }
    pub(crate) fn expire_recovery(&self, id: &str, timeout: Duration) -> Result<bool> {
        Ok(self.db.execute("UPDATE jobs SET state='failed',error='recovery_timed_out',speed=0,eta=NULL WHERE id=?1 AND state='waiting' AND requested='run' AND EXISTS(SELECT 1 FROM recovery_jobs r WHERE r.job_id=jobs.id AND r.generation=jobs.generation AND r.durable>=jobs.durable AND r.since_ms+?2<=?3)",params![id,timeout.as_millis().min(i64::MAX as u128) as i64,now()])?>0)
    }
    pub(crate) fn recovery_info(&self, id: &str) -> Result<Option<(i64, u32, u32)>> {
        Ok(self
            .db
            .query_row(
                "SELECT next_ms,attempts,http_status FROM recovery_jobs WHERE job_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    pub(crate) fn clear_recovery(&self, id: &str) -> Result<()> {
        self.db.execute("DELETE FROM recovery_jobs WHERE job_id=?1 OR job_id IN(SELECT job_id FROM media_components WHERE parent_id=?1)",[id])?;
        Ok(())
    }

    pub(crate) fn schedule_recovery(
        &self,
        id: &str,
        event: &Throttled,
        timeout: Duration,
    ) -> Result<(i64, u32)> {
        let tx = self.write_transaction()?;
        let job = self.get(id)?;
        let previous: Option<(u64, u64, i64, u32)> = tx
            .query_row(
                "SELECT generation,durable,since_ms,attempts FROM recovery_jobs WHERE job_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let at = now();
        let (since, attempt) = match previous {
            Some((generation, durable, since, attempt))
                if generation == job.generation && job.durable <= durable =>
            {
                (since, attempt.saturating_add(1))
            }
            _ => (at, 1),
        };
        if at.saturating_sub(since) as u64 >= timeout.as_millis().min(u64::MAX as u128) as u64 {
            bail!("recovery_timed_out");
        }
        let delay = backoff(attempt, event.delay);
        let deadline = at.saturating_add(delay.as_millis().min(i64::MAX as u128) as i64);
        tx.execute("INSERT INTO recovery_jobs VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(job_id) DO UPDATE SET generation=excluded.generation,durable=excluded.durable,since_ms=excluded.since_ms,attempts=excluded.attempts,next_ms=excluded.next_ms,http_status=excluded.http_status",params![id,job.generation,job.durable,since,attempt,deadline,event.status])?;
        // A successful partial response remains useful. Throttling is governed by
        // a persisted time budget, independently of transport/corruption budgets.
        tx.execute("UPDATE jobs SET state='waiting',error=NULL,speed=0,eta=NULL,received=durable WHERE id=?1 AND requested='run'",[id])?;
        let mut origins = [&job.url, &event.url]
            .into_iter()
            .map(|url| Ok(platform::checked_url(url)?.origin().ascii_serialization()))
            .collect::<Result<Vec<_>>>()?;
        origins.sort();
        origins.dedup();
        let media_parent = self.is_media(id)?;
        for origin in origins {
            tx.execute("INSERT INTO origin_cooldowns VALUES(?1,?2) ON CONFLICT(origin) DO UPDATE SET until_ms=MAX(until_ms,excluded.until_ms)",params![origin,deadline])?;
            // A parent propagates a component's cooldown; it did not make a
            // second failed request, so must not train pacing twice.
            if media_parent {
                continue;
            }
            // Origin learning is independent of the job's no-progress budget:
            // making some progress must not erase a series of rejections.
            let previous: i64 = tx
                .query_row(
                    "SELECT interval_ms FROM origin_pacing WHERE origin=?1",
                    [&origin],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(NORMAL_INTERVAL_MS);
            let interval = previous.saturating_mul(2).clamp(1000, 30000);
            tx.execute("INSERT INTO origin_pacing VALUES(?1,?2,?3,0) ON CONFLICT(origin) DO UPDATE SET next_ms=MAX(next_ms,excluded.next_ms),interval_ms=excluded.interval_ms,successes=0",params![origin,at.saturating_add(interval),interval])?;
        }
        tx.commit()?;
        Ok((deadline, attempt))
    }

    pub(crate) fn admission_wait(&self, url: &str) -> Result<Duration> {
        let origin = platform::checked_url(url)?.origin().ascii_serialization();
        let next: Option<i64> = self
            .db
            .query_row(
                "SELECT next_ms FROM origin_pacing WHERE origin=?1",
                [origin],
                |r| r.get(0),
            )
            .optional()?;
        Ok(Duration::from_millis(
            next.unwrap_or(0).saturating_sub(now()).max(0) as u64,
        ))
    }

    pub(crate) fn admit_request(&self, url: &str) -> Result<Duration> {
        let origin = platform::checked_url(url)?.origin().ascii_serialization();
        let tx = self.request_transaction()?;
        let interval: i64 = tx
            .query_row(
                "SELECT interval_ms FROM origin_pacing WHERE origin=?1",
                [&origin],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(NORMAL_INTERVAL_MS);
        let next = now().saturating_add(interval);
        tx.execute("INSERT INTO origin_pacing VALUES(?1,?2,?3,0) ON CONFLICT(origin) DO UPDATE SET next_ms=excluded.next_ms",params![origin,next,interval])?;
        tx.commit()?;
        Ok(Duration::from_millis(interval as u64))
    }

    pub(crate) fn successful_request(&self, url: &str) -> Result<()> {
        let origin = platform::checked_url(url)?.origin().ascii_serialization();
        self.progress_execute_pacing(&origin)
    }

    pub(crate) fn paced_connections(&self, url: &str, ceiling: usize) -> Result<usize> {
        let origin = platform::checked_url(url)?.origin().ascii_serialization();
        let interval: i64 = self
            .db
            .query_row(
                "SELECT interval_ms FROM origin_pacing WHERE origin=?1",
                [origin],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(NORMAL_INTERVAL_MS);
        Ok(ceiling.min(if interval >= 8000 {
            1
        } else if interval >= 4000 {
            2
        } else {
            ceiling
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_backoff_has_a_floor_and_honors_long_server_deadlines() {
        assert!(backoff(1, None) >= Duration::from_secs(5));
        assert!(backoff(2, None) >= Duration::from_secs(10));
        assert_eq!(
            backoff(1, Some(Duration::from_secs(3600))),
            Duration::from_secs(3600)
        );
        assert!(backoff(100, None) < Duration::from_secs(302));
    }
    fn fixture() -> (tempfile::TempDir, Store, crate::JobView) {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let job = store
            .add(crate::AddJob {
                url: "https://example.org/file".into(),
                destination: root.path().join("file.bin"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        (root, store, job)
    }
    fn throttle() -> Throttled {
        Throttled {
            url: "https://example.org/file".into(),
            delay: Some(Duration::from_secs(600)),
            status: 503,
            invalid_retry_after: false,
        }
    }
    #[test]
    fn repeated_throttles_use_time_budget_without_exhausting_block_budget() {
        let (_root, s, j) = fixture();
        s.db.execute(
            "UPDATE jobs SET generation=3,durable=1048576 WHERE id=?1",
            [&j.id],
        )
        .unwrap();
        for attempt in 1..=5 {
            s.record_throttle(&j.id, 3, 1048576, 1048576).unwrap();
            let (_, n) = s
                .schedule_recovery(&j.id, &throttle(), Duration::from_secs(86400))
                .unwrap();
            assert_eq!(n, attempt);
            assert_eq!(s.get(&j.id).unwrap().durable, 1048576);
        }
        assert_eq!(s.get(&j.id).unwrap().state, "waiting");
        assert_eq!(s.request_counts(&j.id).unwrap().1, "5");
        s.start_range(&j.id, 3, 1048576, 1048576, 0).unwrap();
    }
    #[test]
    fn progress_starts_a_fresh_recovery_window() {
        let (_root, s, j) = fixture();
        s.schedule_recovery(&j.id, &throttle(), Duration::from_secs(86400))
            .unwrap();
        s.db.execute(
            "UPDATE recovery_jobs SET since_ms=0,attempts=20 WHERE job_id=?1",
            [&j.id],
        )
        .unwrap();
        s.db.execute("UPDATE jobs SET durable=1048576 WHERE id=?1", [&j.id])
            .unwrap();
        let (_, attempt) = s
            .schedule_recovery(&j.id, &throttle(), Duration::from_secs(10))
            .unwrap();
        assert_eq!(attempt, 1);
        assert!(!s.recovery_expired(&j.id, Duration::from_secs(10)).unwrap());
    }
    #[test]
    fn cooldown_survives_reopen_and_blocks_same_origin_only() {
        let (root, s, j) = fixture();
        let (deadline, _) = s
            .schedule_recovery(&j.id, &throttle(), Duration::from_secs(86400))
            .unwrap();
        drop(s);
        let s = Store::open(root.path()).unwrap();
        assert_eq!(s.get(&j.id).unwrap().retry_not_before, Some(deadline));
        assert_eq!(s.get(&j.id).unwrap().state, "waiting");
        assert!(s.next_queued().unwrap().is_none());
        let other = s
            .add(crate::AddJob {
                url: "https://other.example/file".into(),
                destination: root.path().join("other.bin"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        assert_eq!(s.next_queued().unwrap(), Some(other.id));
        assert_eq!(
            s.admit_request("https://example.org/other").unwrap(),
            Duration::from_secs(1)
        );
    }
    #[test]
    fn pause_and_cancel_survive_recovery_expiry() {
        for action in ["paused", "canceled"] {
            let (_root, s, j) = fixture();
            s.schedule_recovery(&j.id, &throttle(), Duration::from_secs(86400))
                .unwrap();
            s.request(&j.id, action).unwrap();
            s.db.execute(
                "UPDATE recovery_jobs SET since_ms=0,next_ms=0 WHERE job_id=?1",
                [&j.id],
            )
            .unwrap();
            s.expire_recoveries().unwrap();
            assert_eq!(s.get(&j.id).unwrap().state, action);
            assert!(s.next_queued().unwrap().is_none());
        }
    }
    #[test]
    fn exhausted_time_budget_retains_progress_and_requires_explicit_retry() {
        let (_root, s, j) = fixture();
        s.db.execute("UPDATE jobs SET durable=1048576 WHERE id=?1", [&j.id])
            .unwrap();
        s.schedule_recovery(&j.id, &throttle(), Duration::from_secs(86400))
            .unwrap();
        s.db.execute(
            "UPDATE recovery_jobs SET since_ms=0 WHERE job_id=?1",
            [&j.id],
        )
        .unwrap();
        s.expire_recoveries().unwrap();
        let view = s.view(&j.id).unwrap();
        assert_eq!(view.state, "failed");
        assert_eq!(view.error.as_deref(), Some("recovery_timed_out"));
        assert_eq!(view.durable_bytes, "1048576");
        s.retry_now(&j.id).unwrap();
        assert!(s.recovery_info(&j.id).unwrap().is_none());
    }
    #[test]
    fn pacing_is_shared_and_relaxes_only_after_successful_requests() {
        let (_root, s, j) = fixture();
        let source = "https://example.org/file";
        assert!(s.admission_wait(source).unwrap().is_zero());
        assert_eq!(s.admit_request(source).unwrap(), Duration::from_millis(50));
        s.schedule_recovery(&j.id, &throttle(), Duration::from_secs(86400))
            .unwrap();
        for _ in 0..64 {
            s.successful_request(source).unwrap();
        }
        let interval: i64 =
            s.db.query_row("SELECT interval_ms FROM origin_pacing", [], |r| r.get(0))
                .unwrap();
        assert_eq!(
            interval, 1000,
            "recent throttling survives successful bursts"
        );
        s.db.execute("UPDATE origin_cooldowns SET until_ms=?1", [now() - 120001])
            .unwrap();
        s.successful_request(source).unwrap();
        let interval: i64 =
            s.db.query_row("SELECT interval_ms FROM origin_pacing", [], |r| r.get(0))
                .unwrap();
        assert_eq!(interval, 875, "recovery is gradual after a quiet period");
    }
    #[test]
    fn origin_learning_survives_progress_and_process_reopen() {
        let (root, s, j) = fixture();
        for i in 1..=4 {
            s.db.execute(
                "UPDATE jobs SET durable=?1 WHERE id=?2",
                params![i * 1048576, &j.id],
            )
            .unwrap();
            s.schedule_recovery(&j.id, &throttle(), Duration::from_secs(86400))
                .unwrap();
            assert_eq!(s.recovery_info(&j.id).unwrap().unwrap().1, 1);
            for _ in 0..8 {
                s.successful_request("https://example.org/file").unwrap();
            }
        }
        drop(s);
        let s = Store::open(root.path()).unwrap();
        let interval: i64 =
            s.db.query_row("SELECT interval_ms FROM origin_pacing", [], |r| r.get(0))
                .unwrap();
        assert_eq!(interval, 8000);
        assert_eq!(
            s.paced_connections("https://example.org/file", 4).unwrap(),
            1
        );
        assert_eq!(
            s.paced_connections("https://another.example/file", 4)
                .unwrap(),
            4
        );
    }
    #[test]
    fn automatic_start_never_overrides_a_concurrent_pause_or_cancel() {
        for action in ["paused", "canceled"] {
            let (_root, s, j) = fixture();
            s.schedule_recovery(&j.id, &throttle(), Duration::from_secs(86400))
                .unwrap();
            s.request(&j.id, action).unwrap();
            assert!(!s.begin(&j.id, "waiting", "run").unwrap());
            assert_eq!(s.get(&j.id).unwrap().state, action);
        }
    }
    #[test]
    fn v6_upgrade_preserves_jobs_and_keeps_a_usable_backup() {
        let (root, s, j) = fixture();
        s.db.execute_batch("DROP TABLE recovery_jobs; DROP TABLE origin_pacing; ALTER TABLE policies DROP COLUMN recovery_timeout_seconds; PRAGMA user_version=6;").unwrap();
        drop(s);
        let s = Store::open(root.path()).unwrap();
        assert_eq!(s.get(&j.id).unwrap().policy.recovery_timeout_seconds, 86400);
        let backup = std::fs::read_dir(root.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().starts_with("jobs-v6-"))
            .unwrap()
            .path();
        let old = rusqlite::Connection::open(backup).unwrap();
        assert_eq!(
            old.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            6
        );
        assert_eq!(
            old.query_row("SELECT id FROM jobs", [], |r| r.get::<_, String>(0))
                .unwrap(),
            j.id
        );
        assert_eq!(
            old.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
}
