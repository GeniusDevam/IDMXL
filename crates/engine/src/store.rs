use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::{
    model::{AddJob, Extent, Job, JobView},
    platform,
};

pub struct Store {
    pub(crate) db: Connection,
    pub directory: PathBuf,
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let db = Connection::open(directory.join("jobs.db"))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
        )?;
        let version: u32 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 1 {
            bail!("database_version_unsupported");
        }
        if version == 0 {
            db.execute_batch(include_str!("../migrations/001_initial.sql"))?;
        }
        Ok(Self { db, directory })
    }

    pub fn add(&self, request: AddJob) -> Result<JobView> {
        let url = platform::checked_url(&request.url)?;
        let destination = platform::destination(&request.destination)?;
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
        self.db.execute("INSERT INTO jobs(id,source,display_url,destination,staging,replayable,expected_sha256,created) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id, platform::protect(url.as_str().as_bytes())?, platform::display_url(&url), destination.to_str().context("non_unicode_path")?, staging.to_str().context("non_unicode_path")?, request.replayable, request.expected_sha256.map(|s|s.to_ascii_lowercase()), now()])?;
        self.view(&id)
    }

    pub fn get(&self, id: &str) -> Result<Job> {
        let data = self.db.query_row("SELECT id,source,display_url,destination,staging,replayable,state,generation,total,etag,resolved_url,mode,durable,expected_sha256,actual_sha256,error,requested,publication_file_id FROM jobs WHERE id=?1", [id], |r| {
            Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,bool>(5)?,r.get::<_,String>(6)?,r.get::<_,u64>(7)?,r.get::<_,Option<u64>>(8)?,r.get::<_,Option<String>>(9)?,r.get::<_,Option<Vec<u8>>>(10)?,r.get::<_,String>(11)?,r.get::<_,u64>(12)?,r.get::<_,Option<String>>(13)?,r.get::<_,Option<String>>(14)?,r.get::<_,Option<String>>(15)?,r.get::<_,String>(16)?,r.get::<_,Option<String>>(17)?))
        }).context("job_not_found")?;
        Ok(Job {
            id: data.0,
            url: String::from_utf8(platform::unprotect(&data.1)?)?,
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
        })
    }

    pub fn view(&self, id: &str) -> Result<JobView> {
        Ok(self.get(id)?.into())
    }
    pub fn list(&self) -> Result<Vec<JobView>> {
        let mut statement = self
            .db
            .prepare("SELECT id FROM jobs ORDER BY created DESC,rowid DESC LIMIT 1000")?;
        let ids = statement
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().map(|id| self.view(id)).collect()
    }
    pub fn request(&self, id: &str, action: &str) -> Result<JobView> {
        if !matches!(action, "paused" | "canceled") {
            bail!("invalid_control_action");
        }
        let job = self.get(id)?;
        if matches!(job.state.as_str(), "completed" | "publishing") {
            bail!("job_not_interruptible");
        }
        self.db.execute("UPDATE jobs SET requested=?1,state=CASE WHEN state IN ('downloading','verifying') THEN 'pausing' ELSE ?1 END WHERE id=?2",params![action,id])?;
        self.view(id)
    }
    pub(crate) fn state(&self, id: &str, state: &str, error: Option<&str>) -> Result<()> {
        self.db.execute(
            "UPDATE jobs SET state=?1,error=?2 WHERE id=?3",
            params![state, error, id],
        )?;
        Ok(())
    }
    pub(crate) fn begin(&self, id: &str) -> Result<()> {
        self.db.execute(
            "UPDATE jobs SET state='downloading',requested='run',error=NULL WHERE id=?1",
            [id],
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
                "UPDATE jobs SET generation=generation+1,durable=0,actual_sha256=NULL WHERE id=?1",
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
        transaction.commit()?;
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
        self.db.execute("UPDATE jobs SET state='completed',durable=total,requested='run',error=NULL WHERE id=?1",[id])?;
        Ok(())
    }
    pub(crate) fn recover_active(&self) -> Result<()> {
        self.db.execute("UPDATE jobs SET state=CASE WHEN requested='canceled' THEN 'canceled' ELSE 'paused' END,error='interrupted' WHERE state IN ('downloading','pausing','verifying')",[])?;
        Ok(())
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
