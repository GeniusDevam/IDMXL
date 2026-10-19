use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::{
    model::{AddJob, Extent, Job, JobPolicy, JobView},
    platform,
};

pub struct Store {
    pub(crate) db: Connection,
    pub directory: PathBuf,
    _maintenance: std::fs::File,
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let maintenance = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("maintenance.lock"))?;
        fs2::FileExt::try_lock_shared(&maintenance).context("update_in_progress")?;
        let migration_lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("migration.lock"))?;
        fs2::FileExt::lock_exclusive(&migration_lock)?;
        let db = Connection::open(directory.join("jobs.db"))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
        )?;
        let version: u32 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 6 {
            bail!("database_version_unsupported");
        }
        let idle_locks = if version > 0 && version < 6 {
            let mut locks = Vec::new();
            for name in ["engine.lock", "scheduler.lock"] {
                let lock = std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .read(true)
                    .write(true)
                    .open(directory.join(name))?;
                fs2::FileExt::try_lock_exclusive(&lock)
                    .context("migration_requires_idle_engine")?;
                locks.push(lock);
            }
            locks
        } else {
            Vec::new()
        };
        if version == 0 {
            db.execute_batch(include_str!("../migrations/001_initial.sql"))?;
        }
        if version < 2 {
            if version == 1 {
                // VACUUM INTO includes committed WAL data and leaves the old schema usable.
                let backup = directory.join(format!("jobs-v1-{}.sqlite", Uuid::new_v4()));
                db.execute(
                    "VACUUM INTO ?1",
                    [backup.to_str().context("non_unicode_path")?],
                )?;
            }
            db.execute_batch(include_str!("../migrations/002_runtime_queue.sql"))?;
        }
        if version < 3 {
            if version == 2 {
                let backup = directory.join(format!("jobs-v2-{}.sqlite", Uuid::new_v4()));
                db.execute(
                    "VACUUM INTO ?1",
                    [backup.to_str().context("non_unicode_path")?],
                )?;
            }
            db.execute_batch(include_str!("../migrations/003_origin_deadlines.sql"))?;
            let mut statement = db.prepare("SELECT id,source FROM jobs")?;
            let sources = statement
                .query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(statement);
            for (id, source) in sources {
                let url = String::from_utf8(platform::unprotect(&source)?)?;
                db.execute(
                    "INSERT INTO job_origins VALUES (?1,?2)",
                    params![
                        id,
                        platform::checked_url(&url)?.origin().ascii_serialization()
                    ],
                )?;
            }
            db.execute_batch("COMMIT;")?;
        }
        if version < 4 {
            if version == 3 {
                let backup = directory.join(format!("jobs-v3-{}.sqlite", Uuid::new_v4()));
                db.execute(
                    "VACUUM INTO ?1",
                    [backup.to_str().context("non_unicode_path")?],
                )?;
            }
            db.execute_batch(include_str!("../migrations/004_workflows.sql"))?;
        }
        if version < 5 {
            if version == 4 {
                let backup = directory.join(format!("jobs-v4-{}.sqlite", Uuid::new_v4()));
                db.execute(
                    "VACUUM INTO ?1",
                    [backup.to_str().context("non_unicode_path")?],
                )?;
            }
            db.execute_batch(include_str!("../migrations/005_product_layers.sql"))?;
        }
        if version < 6 {
            if version == 5 {
                let backup = directory.join(format!("jobs-v5-{}.sqlite", Uuid::new_v4()));
                db.execute(
                    "VACUUM INTO ?1",
                    [backup.to_str().context("non_unicode_path")?],
                )?;
            }
            db.execute_batch(include_str!("../migrations/006_everyday.sql"))?;
        }
        drop(idle_locks);
        Ok(Self {
            db,
            directory,
            _maintenance: maintenance,
        })
    }

    pub fn add(&self, request: AddJob) -> Result<JobView> {
        self.insert(request, None, false, "Default", None, None)
    }

    pub(crate) fn add_generated(&self, request: AddJob, queue: &str) -> Result<JobView> {
        self.insert(request, None, true, queue, None, None)
    }

    pub fn add_handoff(&self, request: AddJob, request_id: &str) -> Result<JobView> {
        if Uuid::parse_str(request_id).is_err() {
            bail!("invalid_request_id");
        }
        self.insert(request, Some(request_id), false, "Default", None, None)
    }

    pub(crate) fn insert(
        &self,
        request: AddJob,
        handoff: Option<&str>,
        unique: bool,
        queue: &str,
        capture: Option<(&str, u64, i64)>,
        initial: Option<&str>,
    ) -> Result<JobView> {
        let queue = crate::workflows::checked_queue_name(queue)?;
        let url = platform::checked_url(&request.url)?;
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.db,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        if let Some(request_id) = handoff {
            use rusqlite::OptionalExtension;
            let existing = transaction
                .query_row(
                    "SELECT job_id FROM handoffs WHERE request_id=?1",
                    [request_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?;
            if let Some(id) = existing {
                let job = self.get(&id)?;
                if job.url != url.as_str() {
                    bail!("handoff_id_conflict");
                }
                drop(transaction);
                return self.view(&id);
            }
        }
        let destination = if unique {
            let name = request
                .destination
                .file_name()
                .and_then(|s| s.to_str())
                .context("invalid_filename")?;
            let directory = request
                .destination
                .parent()
                .context("invalid_destination")?;
            let (stem, extension) = name
                .rsplit_once('.')
                .map_or((name, String::new()), |(stem, extension)| {
                    (stem, format!(".{extension}"))
                });
            let mut available = None;
            for number in 1..=10_000 {
                let candidate = directory.join(if number == 1 {
                    name.to_owned()
                } else {
                    format!("{stem} ({number}){extension}")
                });
                if candidate.symlink_metadata().is_ok() {
                    continue;
                }
                let candidate = platform::destination(&candidate)?;
                let used: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM jobs WHERE destination=?1 COLLATE NOCASE)",
                    [candidate.to_str().context("non_unicode_path")?],
                    |r| r.get(0),
                )?;
                if !used {
                    available = Some(candidate);
                    break;
                }
            }
            available.context("no_available_filename")?
        } else {
            let destination = platform::destination(&request.destination)?;
            let used: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs WHERE destination=?1 COLLATE NOCASE)",
                [destination.to_str().context("non_unicode_path")?],
                |r| r.get(0),
            )?;
            if used {
                bail!("destination_in_use");
            }
            destination
        };
        if let Some(digest) = &request.expected_sha256
            && (digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            bail!("invalid_sha256");
        }
        let id = Uuid::new_v4().to_string();
        let staging = destination
            .parent()
            .context("invalid_destination")?
            .join(format!(".idmxl-{id}.part"));
        transaction.execute("INSERT INTO jobs(id,source,display_url,destination,staging,replayable,expected_sha256,created) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id, platform::protect(url.as_str().as_bytes())?, platform::display_url(&url), destination.to_str().context("non_unicode_path")?, staging.to_str().context("non_unicode_path")?, request.replayable, request.expected_sha256.map(|s|s.to_ascii_lowercase()), now()])?;
        transaction.execute(
            "INSERT INTO policies(job_id,queue_name) VALUES (?1,?2)",
            params![id, queue],
        )?;
        if request.replayable {
            transaction.execute("UPDATE policies SET connections=4 WHERE job_id=?1", [&id])?;
            transaction.execute(
                "INSERT INTO settings VALUES(?1,'true')",
                [format!("adaptive:{id}")],
            )?;
        }
        transaction.execute(
            "INSERT INTO job_origins VALUES (?1,?2)",
            params![id, url.origin().ascii_serialization()],
        )?;
        if let Some(request_id) = handoff {
            transaction.execute(
                "INSERT INTO handoffs(request_id,job_id) VALUES (?1,?2)",
                params![request_id, id],
            )?;
            transaction.execute("UPDATE jobs SET public_only=1 WHERE id=?1", [&id])?;
            if let Some((etag, total, expires)) = capture {
                transaction.execute(
                    "INSERT INTO capture_handoffs VALUES(?1,'ready',?2,?3,?4)",
                    params![request_id, expires, etag, total],
                )?;
                transaction.execute(
                    "UPDATE jobs SET state='awaiting_browser',requested='paused' WHERE id=?1",
                    [&id],
                )?;
            }
        }
        if let Some(state) = initial {
            transaction.execute(
                "UPDATE jobs SET state=?2,requested='paused' WHERE id=?1",
                params![id, state],
            )?;
        }
        transaction.commit()?;
        self.view(&id)
    }

    pub fn get(&self, id: &str) -> Result<Job> {
        let data = self.db.query_row("SELECT id,source,display_url,destination,staging,replayable,state,generation,total,etag,resolved_url,mode,durable,expected_sha256,actual_sha256,error,requested,publication_file_id FROM jobs WHERE id=?1", [id], |r| {
            Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,bool>(5)?,r.get::<_,String>(6)?,r.get::<_,u64>(7)?,r.get::<_,Option<u64>>(8)?,r.get::<_,Option<String>>(9)?,r.get::<_,Option<Vec<u8>>>(10)?,r.get::<_,String>(11)?,r.get::<_,u64>(12)?,r.get::<_,Option<String>>(13)?,r.get::<_,Option<String>>(14)?,r.get::<_,Option<String>>(15)?,r.get::<_,String>(16)?,r.get::<_,Option<String>>(17)?))
        }).context("job_not_found")?;
        let runtime = self.db.query_row(
            "SELECT received,speed,eta,public_only FROM jobs WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let source = String::from_utf8(platform::unprotect(&data.1)?)?;
        let retry_not_before = self.retry_deadline(&source)?;
        Ok(Job {
            id: data.0,
            url: source,
            display_url: data.2,
            destination: data.3.into(),
            staging: data.4.into(),
            replayable: data.5,
            state: data.6,
            generation: data.7,
            total: data.8,
            etag: data.9,
            resolved_url: data
                .10
                .map(|s| platform::unprotect(&s).and_then(|bytes| Ok(String::from_utf8(bytes)?)))
                .transpose()?,
            mode: data.11,
            durable: data.12,
            expected_sha256: data.13,
            actual_sha256: data.14,
            error: data.15,
            requested: data.16,
            publication_file_id: data.17,
            received: runtime.0,
            speed: runtime.1,
            eta: runtime.2,
            public_only: runtime.3,
            policy: self.policy(id)?,
            retry_not_before,
        })
    }

    pub fn view(&self, id: &str) -> Result<JobView> {
        let mut view: JobView = self.get(id)?.into();
        view.created_ms = self
            .db
            .query_row("SELECT created FROM jobs WHERE id=?1", [id], |r| r.get(0))?;
        view.queue_window_open = self.queue_open(&view.policy.queue_name)?;
        (view.transfer_requests, view.failed_requests) = self.request_counts(id)?;
        Ok(view)
    }
    pub fn list(&self) -> Result<Vec<JobView>> {
        let mut statement = self
            .db
            .prepare("SELECT id FROM jobs WHERE state!='media_component' AND id NOT IN (SELECT job_id FROM media_components) ORDER BY created DESC,rowid DESC LIMIT 1000")?;
        let ids = statement
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().map(|id| self.view(id)).collect()
    }
    pub fn request(&self, id: &str, action: &str) -> Result<JobView> {
        self.capture_runnable(id)?;
        if !matches!(action, "paused" | "canceled" | "queued") {
            bail!("invalid_control_action");
        }
        let job = self.get(id)?;
        if action == "queued" && job.state == "failed" {
            self.reset_retry_budget(id)?;
        }
        if job.state == "preparing_media" && action != "canceled" {
            bail!("media_preparation_incomplete");
        }
        if matches!(job.state.as_str(), "completed" | "publishing" | "canceled")
            || action == "queued"
                && matches!(job.state.as_str(), "downloading" | "pausing" | "verifying")
        {
            bail!("job_not_interruptible");
        }
        self.db.execute("UPDATE jobs SET requested=?1,state=CASE WHEN state IN ('downloading','verifying') THEN 'pausing' ELSE ?1 END WHERE id=?2",params![action,id])?;
        self.view(id)
    }
    pub(crate) fn state(&self, id: &str, state: &str, error: Option<&str>) -> Result<()> {
        self.db.execute(
            "UPDATE jobs SET state=?1,error=?2,speed=0,eta=NULL WHERE id=?3",
            params![state, error, id],
        )?;
        Ok(())
    }
    pub(crate) fn begin(&self, id: &str) -> Result<()> {
        self.db.execute(
            "UPDATE jobs SET state='downloading',requested='run',error=NULL,received=durable,speed=0,eta=NULL,last_started=?2 WHERE id=?1",
            params![id,now()],
        )?;
        Ok(())
    }
    pub(crate) fn requested(&self, id: &str) -> Result<String> {
        Ok(self
            .db
            .query_row("SELECT requested FROM jobs WHERE id=?1", [id], |r| r.get(0))?)
    }
    pub(crate) fn identity(
        &self,
        id: &str,
        total: Option<u64>,
        etag: Option<&str>,
        resolved: &str,
        mode: &str,
        reset: bool,
    ) -> Result<()> {
        let transaction = self.db.unchecked_transaction()?;
        if reset {
            transaction.execute("DELETE FROM extents WHERE job_id=?1", [id])?;
            transaction.execute(
                "UPDATE jobs SET generation=generation+1,durable=0,received=0,actual_sha256=NULL WHERE id=?1",
                [id],
            )?;
        }
        transaction.execute(
            "UPDATE jobs SET total=?1,etag=?2,resolved_url=?3,mode=?4 WHERE id=?5",
            params![
                total,
                etag,
                platform::protect(resolved.as_bytes())?,
                mode,
                id
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }
    pub(crate) fn extents(&self, id: &str, generation: u64) -> Result<Vec<Extent>> {
        let mut statement=self.db.prepare("SELECT offset,length,digest FROM extents WHERE job_id=?1 AND generation=?2 ORDER BY offset")?;
        Ok(statement
            .query_map(params![id, generation], |r| {
                Ok(Extent {
                    offset: r.get(0)?,
                    length: r.get(1)?,
                    digest: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub(crate) fn forget_extent(&self, id: &str, offset: u64) -> Result<()> {
        let transaction = self.db.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM extents WHERE job_id=?1 AND offset=?2",
            params![id, offset],
        )?;
        transaction.execute("UPDATE jobs SET durable=COALESCE((SELECT SUM(length) FROM extents WHERE job_id=?1),0) WHERE id=?1",[id])?;
        transaction.commit()?;
        Ok(())
    }
    pub(crate) fn checkpoint(&self, id: &str, generation: u64, extent: &Extent) -> Result<()> {
        let transaction = self.db.unchecked_transaction()?;
        let current: u64 =
            transaction.query_row("SELECT generation FROM jobs WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        if current != generation {
            bail!("stale_extent_generation");
        }
        transaction.execute("INSERT OR REPLACE INTO extents(job_id,generation,offset,length,digest) VALUES(?1,?2,?3,?4,?5)",params![id,generation,extent.offset,extent.length,extent.digest])?;
        transaction.execute("UPDATE jobs SET durable=(SELECT COALESCE(SUM(length),0) FROM extents WHERE job_id=?1 AND generation=?2) WHERE id=?1",params![id,generation])?;
        crate::fault::point("before_extent_commit");
        transaction.commit()?;
        crate::fault::point("after_extent_commit");
        Ok(())
    }
    pub(crate) fn publication_intent(
        &self,
        id: &str,
        total: u64,
        digest: &str,
        file_id: &str,
    ) -> Result<()> {
        self.db.execute("UPDATE jobs SET state='publishing',total=?1,actual_sha256=?2,publication_file_id=?3 WHERE id=?4",params![total,digest,file_id,id])?;
        Ok(())
    }
    pub(crate) fn complete(&self, id: &str) -> Result<()> {
        let transaction = self.db.unchecked_transaction()?;
        transaction.execute("UPDATE jobs SET state='completed',durable=total,received=total,speed=0,eta=NULL,requested='run',error=NULL WHERE id=?1",[id])?;
        transaction.execute("INSERT OR IGNORE INTO completion_events(job_id,created) SELECT ?1,?2 WHERE NOT EXISTS(SELECT 1 FROM media_components WHERE job_id=?1)",params![id,now()])?;
        transaction.commit()?;
        Ok(())
    }
    pub(crate) fn recover_active(&self) -> Result<()> {
        self.db.execute("UPDATE jobs SET state=CASE WHEN requested='canceled' THEN 'canceled' ELSE 'paused' END,error='interrupted',received=durable,speed=0,eta=NULL WHERE state IN ('downloading','pausing','verifying')",[])?;
        Ok(())
    }

    pub(crate) fn progress(
        &self,
        id: &str,
        received: u64,
        speed: u64,
        total: Option<u64>,
    ) -> Result<()> {
        let eta = total
            .filter(|_| speed > 0)
            .map(|n| n.saturating_sub(received).div_ceil(speed));
        self.db.execute(
            "UPDATE jobs SET received=?1,speed=?2,eta=?3,total=COALESCE(total,?4) WHERE id=?5",
            params![received, speed, eta, total, id],
        )?;
        Ok(())
    }

    pub fn policy(&self, id: &str) -> Result<JobPolicy> {
        let mut policy = self.db.query_row("SELECT queue_name,priority,not_before,deadline,connections,retries,speed_limit FROM policies WHERE job_id=?1", [id], |r| Ok(JobPolicy {
            queue_name:r.get(0)?,priority:r.get(1)?,not_before:r.get(2)?,deadline:r.get(3)?,connections:r.get(4)?,retries:r.get(5)?,speed_limit:r.get(6)?,adaptive:false
        }))?;
        policy.adaptive = self.setting_bool(&format!("adaptive:{id}"), false)?;
        Ok(policy)
    }

    pub fn set_policy(&self, id: &str, policy: &JobPolicy) -> Result<JobView> {
        if policy.queue_name.trim().is_empty()
            || policy.queue_name.len() > 64
            || policy.queue_name.chars().any(char::is_control)
            || !(-1000..=1000).contains(&policy.priority)
            || !(1..=4).contains(&policy.connections)
            || policy.retries > 5
            || policy.not_before < 0
            || policy.deadline.is_some_and(|n| n <= policy.not_before)
            || policy.speed_limit > i64::MAX as u64
        {
            bail!("invalid_queue_policy");
        }
        self.get(id)?;
        let transaction = self.db.unchecked_transaction()?;
        transaction.execute("UPDATE policies SET queue_name=?1,priority=?2,not_before=?3,deadline=?4,connections=?5,retries=?6,speed_limit=?7 WHERE job_id=?8",
            params![policy.queue_name.trim(),policy.priority,policy.not_before,policy.deadline,policy.connections,policy.retries,policy.speed_limit,id])?;
        transaction.execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("adaptive:{id}"),policy.adaptive.to_string()])?;
        transaction.commit()?;
        self.view(id)
    }

    pub fn set_queue_running(&self, running: bool) -> Result<()> {
        self.db.execute(
            "UPDATE settings SET value=?1 WHERE key='queue_running'",
            [running.to_string()],
        )?;
        Ok(())
    }
    pub fn queue_running(&self) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT value FROM settings WHERE key='queue_running'",
            [],
            |r| r.get::<_, String>(0),
        )? == "true")
    }
    pub fn global_speed_limit(&self) -> Result<u64> {
        Ok(self
            .db
            .query_row(
                "SELECT value FROM settings WHERE key='global_speed_limit'",
                [],
                |r| r.get::<_, String>(0),
            )?
            .parse()?)
    }
    pub fn set_global_speed_limit(&self, limit: u64) -> Result<()> {
        if limit > i64::MAX as u64 {
            bail!("invalid_speed_limit");
        }
        self.db.execute(
            "UPDATE settings SET value=?1 WHERE key='global_speed_limit'",
            [limit.to_string()],
        )?;
        Ok(())
    }
    pub(crate) fn next_queued(&self) -> Result<Option<String>> {
        let mut statement = self.db.prepare("SELECT j.id,p.queue_name,j.state FROM jobs j JOIN policies p ON p.job_id=j.id JOIN job_origins o ON o.job_id=j.id LEFT JOIN origin_cooldowns c ON c.origin=o.origin WHERE j.state IN ('queued','publishing') AND p.not_before<=?1 AND (p.deadline IS NULL OR p.deadline>?1) AND (c.until_ms IS NULL OR c.until_ms<=?1) ORDER BY (SELECT MAX(j2.last_started) FROM jobs j2 JOIN policies p2 ON p2.job_id=j2.id WHERE p2.queue_name=p.queue_name),p.priority DESC,j.last_started,j.created,j.rowid")?;
        let candidates = statement.query_map([now()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for candidate in candidates {
            let (id, queue, state) = candidate?;
            // Publication recovery does not fetch a new body and must not wait for a window.
            if state == "publishing" || self.queue_open(&queue)? {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    pub fn retry_deadline(&self, url: &str) -> Result<Option<i64>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .db
            .query_row(
                "SELECT until_ms FROM origin_cooldowns WHERE origin=?1 AND until_ms>?2",
                params![
                    platform::checked_url(url)?.origin().ascii_serialization(),
                    now()
                ],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub(crate) fn defer_origin(&self, url: &str, delay: std::time::Duration) -> Result<()> {
        let deadline = now().saturating_add(delay.as_millis().min(i64::MAX as u128) as i64);
        self.db.execute("INSERT INTO origin_cooldowns VALUES (?1,?2) ON CONFLICT(origin) DO UPDATE SET until_ms=MAX(until_ms,excluded.until_ms)",params![platform::checked_url(url)?.origin().ascii_serialization(),deadline])?;
        Ok(())
    }
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v3_upgrade_waits_for_scheduler_and_preserves_jobs_in_backup() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let job = store
            .add(AddJob {
                url: "https://example.org/file?token=protected".into(),
                destination: root.path().join("result.bin"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        store.db.execute_batch("DROP TABLE request_attempts; DROP TABLE completion_events; DROP TABLE media_components; DROP TABLE media_jobs; DROP TABLE capture_handoffs; DROP TABLE queue_windows; DROP INDEX jobs_destination_lookup; PRAGMA user_version=3;").unwrap();
        drop(store);
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.path().join("scheduler.lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        assert_eq!(
            Store::open(root.path()).err().unwrap().to_string(),
            "migration_requires_idle_engine"
        );
        drop(lock);
        let upgraded = Store::open(root.path()).unwrap();
        assert_eq!(
            upgraded.view(&job.id).unwrap().source,
            "https://example.org/file"
        );
        assert_eq!(
            upgraded
                .db
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            6
        );
        let path = std::fs::read_dir(root.path())
            .unwrap()
            .filter_map(|p| p.ok())
            .find(|p| p.file_name().to_string_lossy().starts_with("jobs-v3-"))
            .unwrap()
            .path();
        let old = Connection::open(path).unwrap();
        assert_eq!(
            old.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            old.query_row("SELECT id FROM jobs", [], |r| r.get::<_, String>(0))
                .unwrap(),
            job.id
        );
        assert_eq!(
            old.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }

    #[test]
    fn migration_waits_for_idle_engine_and_backs_up_old_schema() {
        let root = tempfile::tempdir().unwrap();
        let db = Connection::open(root.path().join("jobs.db")).unwrap();
        db.execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        db.execute_batch(include_str!("../migrations/002_runtime_queue.sql"))
            .unwrap();
        drop(db);
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.path().join("engine.lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        let error = Store::open(root.path())
            .err()
            .expect("migration must not run beside an old writer");
        assert_eq!(error.to_string(), "migration_requires_idle_engine");
        drop(lock);
        let store = Store::open(root.path()).unwrap();
        assert_eq!(
            store
                .db
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            6
        );
        let backup = std::fs::read_dir(root.path())
            .unwrap()
            .filter_map(|n| n.ok())
            .find(|n| n.file_name().to_string_lossy().starts_with("jobs-v2-"))
            .unwrap();
        let old = Connection::open(backup.path()).unwrap();
        assert_eq!(
            old.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            old.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
}
