//! Two-phase public-file takeover. A prepared job cannot run through any entry point.
use crate::{AddJob, Store, platform, transport};
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

const LEASE_MS: i64 = 60_000;
#[derive(Debug, Serialize)]
pub struct CaptureStatus {
    pub request_id: String,
    pub job_id: String,
    pub phase: String,
    pub expires: i64,
}

impl Store {
    pub fn capture_status(&self, request_id: &str) -> Result<CaptureStatus> {
        let (id, phase, expires): (String,String,i64) = self.db.query_row(
            "SELECT h.job_id,c.phase,c.expires FROM capture_handoffs c JOIN handoffs h USING(request_id) WHERE request_id=?1",
            [request_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).context("handoff_not_found")?;
        Ok(CaptureStatus {
            request_id: request_id.into(),
            job_id: id,
            phase: if phase == "ready" && expires <= crate::store::now() {
                "expired".into()
            } else {
                phase
            },
            expires,
        })
    }

    pub fn prepare_capture(
        &self,
        request_id: &str,
        url: &str,
        filename: &str,
        directory: &Path,
        etag: &str,
        total: u64,
    ) -> Result<CaptureStatus> {
        uuid::Uuid::parse_str(request_id).context("invalid_request_id")?;
        let source = platform::checked_url(url)?;
        if source.scheme() != "https"
            || source.query().is_some()
            || source.fragment().is_some()
            || total == 0
            || total > i64::MAX as u64
            || etag.len() < 2
            || etag.len() > 1024
            || !etag.starts_with('"')
            || !etag.ends_with('"')
            || etag[1..etag.len() - 1].contains(['"', '\r', '\n'])
        {
            bail!("capture_ineligible");
        }
        if let Ok(status) = self.capture_status(request_id) {
            let saved: (String, u64) = self.db.query_row(
                "SELECT etag,total FROM capture_handoffs WHERE request_id=?1",
                [request_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if self.get(&status.job_id)?.url != source.as_str() || saved != (etag.into(), total) {
                bail!("handoff_id_conflict");
            }
            if status.phase == "committed" {
                return Ok(status);
            }
            if status.phase == "aborted" {
                bail!("handoff_aborted");
            }
        } else {
            let exists: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM handoffs WHERE request_id=?1)",
                [request_id],
                |r| r.get(0),
            )?;
            if exists {
                bail!("handoff_id_conflict");
            }
        }
        // No cookies, proxy, credentials or redirects qualify. DNS is pinned to public addresses.
        let probe =
            transport::probe_exact(source.as_str(), Arc::new(AtomicBool::new(false)), true)?;
        if !probe.ranged
            || probe.resolved != source.as_str()
            || probe.etag.as_deref() != Some(etag)
            || probe.total != Some(total)
        {
            bail!("capture_identity_mismatch");
        }
        let expires = crate::store::now() + LEASE_MS;
        self.insert(
            AddJob {
                url: source.into(),
                destination: directory.join(format!("{request_id}-{filename}")),
                replayable: true,
                expected_sha256: None,
            },
            Some(request_id),
            false,
            "Default",
            Some((etag, total, expires)),
            None,
        )?;
        // Repeated preparation can renew only an existing uncommitted lease.
        self.db.execute(
            "UPDATE capture_handoffs SET expires=?2 WHERE request_id=?1 AND phase='ready'",
            params![request_id, expires],
        )?;
        self.capture_status(request_id)
    }

    pub fn commit_capture(&self, request_id: &str) -> Result<CaptureStatus> {
        let tx = self.db.unchecked_transaction()?;
        let status = self.capture_status(request_id)?;
        match status.phase.as_str() {
            "committed" => {}
            "ready" => {
                tx.execute("UPDATE jobs SET state='queued',requested='run' WHERE id=?1 AND state='awaiting_browser'",[&status.job_id])?;
                tx.execute(
                    "UPDATE capture_handoffs SET phase='committed' WHERE request_id=?1",
                    [request_id],
                )?;
            }
            _ => bail!("handoff_not_ready"),
        }
        tx.commit()?;
        self.capture_status(request_id)
    }

    pub fn abort_capture(&self, request_id: &str) -> Result<CaptureStatus> {
        let tx = self.db.unchecked_transaction()?;
        let status = self.capture_status(request_id)?;
        if status.phase == "committed" {
            bail!("handoff_already_committed");
        }
        tx.execute("UPDATE jobs SET state='canceled',requested='canceled' WHERE id=?1 AND state='awaiting_browser'",[&status.job_id])?;
        tx.execute(
            "UPDATE capture_handoffs SET phase='aborted' WHERE request_id=?1",
            [request_id],
        )?;
        tx.commit()?;
        self.capture_status(request_id)
    }

    pub(crate) fn capture_runnable(&self, id: &str) -> Result<()> {
        let phase:Option<String>=self.db.query_row("SELECT c.phase FROM capture_handoffs c JOIN handoffs h USING(request_id) WHERE h.job_id=?1",[id],|r|r.get(0)).optional()?;
        if phase.is_some_and(|p| p != "committed") {
            bail!("browser_handoff_pending");
        }
        Ok(())
    }

    pub(crate) fn capture_identity(
        &self,
        id: &str,
        probe: Option<&transport::Probe>,
    ) -> Result<()> {
        let identity:Option<(String,u64)>=self.db.query_row("SELECT c.etag,c.total FROM capture_handoffs c JOIN handoffs h USING(request_id) WHERE h.job_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((etag, total)) = identity {
            let probe = probe.context("capture_identity_mismatch")?;
            if !probe.ranged
                || probe.etag.as_ref() != Some(&etag)
                || probe.total != Some(total)
                || probe.resolved != self.get(id)?.url
            {
                bail!("resource_changed");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepared(store: &Store, root: &Path) -> CaptureStatus {
        let request_id = uuid::Uuid::new_v4().to_string();
        store
            .insert(
                AddJob {
                    url: "https://example.org/file".into(),
                    destination: root.join(format!("{request_id}.bin")),
                    replayable: true,
                    expected_sha256: None,
                },
                Some(&request_id),
                false,
                "Default",
                Some(("\"one\"", 100, crate::store::now() + 60000)),
                None,
            )
            .unwrap();
        store.capture_status(&request_id).unwrap()
    }
    #[test]
    fn prepared_jobs_cannot_run_until_a_durable_idempotent_commit() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let status = prepared(&store, root.path());
        assert_eq!(store.get(&status.job_id).unwrap().state, "awaiting_browser");
        assert!(store.request(&status.job_id, "queued").is_err());
        let engine = crate::Engine::new(store);
        assert_eq!(
            engine
                .run(&status.job_id, Default::default())
                .unwrap_err()
                .to_string(),
            "browser_handoff_pending"
        );
        engine.store.commit_capture(&status.request_id).unwrap();
        drop(engine);
        let store = Store::open(root.path()).unwrap();
        assert_eq!(
            store.commit_capture(&status.request_id).unwrap().job_id,
            status.job_id
        );
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.get(&status.job_id).unwrap().state, "queued");
        assert!(store.abort_capture(&status.request_id).is_err());
    }
    #[test]
    fn expired_and_aborted_leases_never_enter_the_queue() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let status = prepared(&store, root.path());
        store
            .db
            .execute("UPDATE capture_handoffs SET expires=0", [])
            .unwrap();
        assert_eq!(
            store.capture_status(&status.request_id).unwrap().phase,
            "expired"
        );
        assert!(store.commit_capture(&status.request_id).is_err());
        store.abort_capture(&status.request_id).unwrap();
        assert_eq!(store.get(&status.job_id).unwrap().state, "canceled");
        assert!(store.commit_capture(&status.request_id).is_err());
    }
    #[test]
    fn capture_never_silently_changes_browser_representation() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let status = prepared(&store, root.path());
        for (etag, total, resolved) in [
            ("\"two\"", 100, "https://example.org/file"),
            ("\"one\"", 101, "https://example.org/file"),
            ("\"one\"", 100, "https://example.org/elsewhere"),
        ] {
            assert!(
                store
                    .capture_identity(
                        &status.job_id,
                        Some(&transport::Probe {
                            ranged: true,
                            etag: Some(etag.into()),
                            total: Some(total),
                            resolved: resolved.into()
                        })
                    )
                    .is_err()
            );
        }
        assert!(
            store
                .prepare_capture(
                    &uuid::Uuid::new_v4().to_string(),
                    "https://example.org/file?token=one-use",
                    "file.bin",
                    root.path(),
                    "\"one\"",
                    100
                )
                .is_err()
        );
    }
}
