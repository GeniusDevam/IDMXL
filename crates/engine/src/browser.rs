//! Browser-owned transfers: metadata and explicit controls, never source replay.
use crate::{JobPolicy, JobView, Store};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

const RECORD: &str = "browser_transfer:";
const COMMAND: &str = "browser_command:";
const CAPACITY: usize = 1000;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserReport {
    pub id: String,
    pub name: String,
    pub origin: Option<String>,
    pub state: String,
    pub total: Option<u64>,
    pub received: u64,
    pub speed: u64,
    pub can_resume: bool,
    pub exists: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> BrowserReport {
        BrowserReport {
            id: uuid::Uuid::new_v4().to_string(),
            name: "browser-file.zip".into(),
            origin: Some("https://example.org".into()),
            state: "downloading".into(),
            total: None,
            received: 123,
            speed: 10,
            can_resume: true,
            exists: false,
        }
    }
    #[test]
    fn browser_history_never_becomes_a_native_replay_or_enables_the_queue() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let client = uuid::Uuid::new_v4().to_string();
        let mut item = report();
        store
            .browser_sync(&client, std::slice::from_ref(&item), &[])
            .unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(!store.queue_running().unwrap());
        let job = store.desktop_jobs().unwrap().pop().unwrap();
        assert_eq!(job.mode, "browser");
        assert_eq!(job.total_bytes, None);
        assert_eq!(job.sha256, None);
        assert!(store.get(&job.id).is_err());
        assert!(store.request(&job.id, "queued").is_err());
        assert!(store.browser_needs_interface(&client).unwrap());
        store.browser_interface_requested(&client).unwrap();
        item.state = "completed".into();
        item.total = Some(123);
        item.exists = true;
        store.browser_sync(&client, &[item], &[]).unwrap();
        drop(store);
        let store = Store::open(root.path()).unwrap();
        let jobs = store.desktop_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].state, "completed");
        assert!(jobs[0].file_available);
        assert!(!store.browser_needs_interface(&client).unwrap());
    }
    #[test]
    fn browser_controls_are_owned_acknowledged_and_never_execute_native_jobs() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let client = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        let item = report();
        store
            .browser_sync(&client, std::slice::from_ref(&item), &[])
            .unwrap();
        store.browser_control(&item.id, "pause").unwrap();
        assert!(store.browser_sync(&other, &[], &[]).unwrap().is_empty());
        assert!(
            store
                .browser_sync(&other, std::slice::from_ref(&item), &[])
                .is_err()
        );
        let commands = store.browser_sync(&client, &[], &[]).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].action, "pause");
        store
            .browser_sync(
                &other,
                &[],
                &[BrowserAck {
                    id: commands[0].id.clone(),
                    success: true,
                }],
            )
            .unwrap();
        assert_eq!(store.browser_sync(&client, &[], &[]).unwrap().len(), 1);
        assert!(
            store
                .browser_sync(
                    &client,
                    &[],
                    &[BrowserAck {
                        id: commands[0].id.clone(),
                        success: false
                    }]
                )
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store.browser_views().unwrap()[0].error.as_deref(),
            Some("browser_control_failed")
        );
        assert!(store.browser_control(&item.id, "execute").is_err());
        assert!(!store.queue_running().unwrap());
    }
    #[test]
    fn invalid_reports_roll_back_the_batch_without_storing_urls_or_paths() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let client = uuid::Uuid::new_v4().to_string();
        let good = report();
        for invalid in [
            "https://example.org/private?token=secret",
            "https://user:password@example.org",
        ] {
            let mut bad = report();
            bad.origin = Some(invalid.into());
            assert!(
                store
                    .browser_sync(&client, &[good.clone(), bad], &[])
                    .is_err()
            );
            assert!(store.browser_views().unwrap().is_empty());
        }
        let mut bad = report();
        bad.name = "C:\\private\\file.zip".into();
        assert!(store.browser_sync(&client, &[bad], &[]).is_err());
        assert!(store.browser_sync(&client, &vec![good; 65], &[]).is_err());
        assert!(store.browser_views().unwrap().is_empty());
    }
    #[test]
    fn stale_browser_connections_are_visible_and_progress_does_not_claim_a_hash() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let client = uuid::Uuid::new_v4().to_string();
        let item = report();
        store
            .browser_sync(&client, std::slice::from_ref(&item), &[])
            .unwrap();
        let mut record = store.browser_record(&item.id).unwrap().unwrap();
        record.updated = 0;
        store.save_browser_record(&record).unwrap();
        let view = store.browser_views().unwrap().pop().unwrap();
        assert_eq!(view.state, "paused");
        assert_eq!(view.error.as_deref(), Some("browser_disconnected"));
        assert_eq!(view.sha256, None);
        let mut current = item;
        current.state = "attention".into();
        current.exists = true;
        store.browser_sync(&client, &[current], &[]).unwrap();
        let view = store.browser_views().unwrap().pop().unwrap();
        assert!(!view.file_available);
        assert_eq!(view.error.as_deref(), Some("browser_attention"));
    }

    #[test]
    fn browser_reports_cannot_alias_native_jobs_and_expired_commands_fail_visibly() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let client = uuid::Uuid::new_v4().to_string();
        let native = store
            .add(crate::AddJob {
                url: "https://example.org/file".into(),
                destination: root.path().join("native.zip"),
                replayable: false,
                expected_sha256: None,
            })
            .unwrap();
        let mut item = report();
        item.id = native.id.clone();
        assert!(
            store
                .browser_sync(&client, std::slice::from_ref(&item), &[])
                .is_err()
        );
        assert!(store.browser_views().unwrap().is_empty());
        item.id = uuid::Uuid::new_v4().to_string();
        store
            .browser_sync(&client, std::slice::from_ref(&item), &[])
            .unwrap();
        store.browser_control(&item.id, "pause").unwrap();
        let mut command = store.browser_sync(&client, &[], &[]).unwrap().remove(0);
        command.created = 0;
        store
            .db
            .execute(
                "UPDATE settings SET value=?1 WHERE key=?2",
                params![
                    serde_json::to_string(&command).unwrap(),
                    format!("{COMMAND}{client}:{}", command.id)
                ],
            )
            .unwrap();
        assert!(store.browser_sync(&client, &[], &[]).unwrap().is_empty());
        assert_eq!(
            store.browser_views().unwrap()[0].error.as_deref(),
            Some("browser_control_failed")
        );
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.view(&native.id).unwrap().state, native.state);
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct Record {
    client: String,
    report: BrowserReport,
    updated: i64,
    created: i64,
    shown: bool,
    control_error: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct BrowserCommand {
    pub id: String,
    pub transfer_id: String,
    pub action: String,
    pub created: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserAck {
    pub id: String,
    pub success: bool,
}

fn uuid(value: &str) -> Result<()> {
    if uuid::Uuid::parse_str(value)?.to_string() != value {
        bail!("invalid_browser_message");
    }
    Ok(())
}

impl Store {
    fn browser_records(&self) -> Result<Vec<Record>> {
        let mut query = self
            .db
            .prepare("SELECT value FROM settings WHERE key LIKE 'browser_transfer:%'")?;
        let values = query
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .into_iter()
            .map(|value| Ok(serde_json::from_str(&value)?))
            .collect()
    }

    fn browser_record(&self, id: &str) -> Result<Option<Record>> {
        uuid(id)?;
        let value: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [format!("{RECORD}{id}")],
                |r| r.get(0),
            )
            .optional()?;
        value.map(|v| Ok(serde_json::from_str(&v)?)).transpose()
    }

    fn save_browser_record(&self, record: &Record) -> Result<()> {
        self.db.execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![format!("{RECORD}{}",record.report.id),serde_json::to_string(record)?])?;
        Ok(())
    }

    pub fn is_browser_transfer(&self, id: &str) -> Result<bool> {
        Ok(self.browser_record(id)?.is_some())
    }

    pub fn browser_views(&self) -> Result<Vec<JobView>> {
        let mut records = self.browser_records()?;
        records.sort_by_key(|record| std::cmp::Reverse(record.updated));
        Ok(records
            .into_iter()
            .map(|record| {
                let report = record.report;
                let connected = crate::store::now() - record.updated < 15_000;
                let error = if record.control_error {
                    Some("browser_control_failed".into())
                } else if !connected
                    && !["completed", "canceled", "failed"].contains(&report.state.as_str())
                {
                    Some("browser_disconnected".into())
                } else if report.state == "attention" {
                    Some("browser_attention".into())
                } else if report.state == "failed" {
                    Some("browser_interrupted".into())
                } else {
                    None
                };
                let file_available = report.exists && report.state == "completed";
                JobView {
                    created_ms: record.created,
                    id: report.id,
                    source: report.origin.unwrap_or_else(|| "Browser download".into()),
                    destination: "Managed by the browser".into(),
                    name: report.name,
                    state: if !connected && report.state == "downloading" {
                        "paused".into()
                    } else {
                        report.state
                    },
                    generation: 0,
                    total_bytes: report.total.map(|n| n.to_string()),
                    durable_bytes: report.received.to_string(),
                    received_bytes: report.received.to_string(),
                    speed_bytes_per_second: if connected { report.speed } else { 0 }.to_string(),
                    eta_seconds: None,
                    policy: JobPolicy::default(),
                    retry_not_before: None,
                    mode: "browser".into(),
                    replayable: report.can_resume,
                    sha256: None,
                    error,
                    file_available,
                    queue_window_open: true,
                    transfer_requests: "0".into(),
                    failed_requests: "0".into(),
                }
            })
            .collect())
    }

    pub fn browser_sync(
        &self,
        client: &str,
        reports: &[BrowserReport],
        acknowledgements: &[BrowserAck],
    ) -> Result<Vec<BrowserCommand>> {
        uuid(client)?;
        if reports.len() > 64 || acknowledgements.len() > 256 {
            bail!("invalid_browser_message");
        }
        let tx = self.db.unchecked_transaction()?;
        for report in reports {
            uuid(&report.id)?;
            let native_exists: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1)",
                [&report.id],
                |r| r.get(0),
            )?;
            if native_exists {
                bail!("browser_client_conflict");
            }
            if report.name.is_empty()
                || report.name.len() > 1024
                || report.name.chars().any(char::is_control)
                || report.name.contains(['/', '\\'])
                || ![
                    "downloading",
                    "paused",
                    "completed",
                    "failed",
                    "canceled",
                    "attention",
                ]
                .contains(&report.state.as_str())
                || report.total.is_some_and(|n| n > i64::MAX as u64)
                || report.received > i64::MAX as u64
                || report.speed > i64::MAX as u64
            {
                bail!("invalid_browser_message");
            }
            if let Some(origin) = &report.origin {
                let parsed = crate::platform::checked_url(origin)?;
                if parsed.origin().ascii_serialization() != *origin {
                    bail!("invalid_browser_message");
                }
            }
            let saved = self.browser_record(&report.id)?;
            if saved.as_ref().is_some_and(|record| record.client != client) {
                bail!("browser_client_conflict");
            }
            if saved.is_none() {
                let mut records = self.browser_records()?;
                if records.len() >= CAPACITY {
                    records.sort_by_key(|r| r.updated);
                    let Some(old) = records.iter().find(|r| {
                        ["completed", "canceled", "failed"].contains(&r.report.state.as_str())
                    }) else {
                        bail!("browser_capacity");
                    };
                    self.db.execute(
                        "DELETE FROM settings WHERE key=?1",
                        [format!("{RECORD}{}", old.report.id)],
                    )?;
                }
            }
            self.save_browser_record(&Record {
                client: client.into(),
                report: report.clone(),
                updated: crate::store::now(),
                created: saved.as_ref().map_or_else(crate::store::now, |r| r.created),
                shown: saved.as_ref().is_some_and(|r| r.shown),
                control_error: saved.is_some_and(|r| r.control_error),
            })?;
        }
        let prefix = format!("{COMMAND}{client}:");
        for ack in acknowledgements {
            uuid(&ack.id)?;
            let key = format!("{prefix}{}", ack.id);
            let value: Option<String> = self
                .db
                .query_row("SELECT value FROM settings WHERE key=?1", [&key], |r| {
                    r.get(0)
                })
                .optional()?;
            if let Some(value) = value {
                let command: BrowserCommand = serde_json::from_str(&value)?;
                if let Some(mut record) = self.browser_record(&command.transfer_id)? {
                    if record.client != client {
                        bail!("browser_client_conflict");
                    }
                    record.control_error = !ack.success;
                    self.save_browser_record(&record)?;
                }
                self.db
                    .execute("DELETE FROM settings WHERE key=?1", [key])?;
            }
        }
        let mut query = self
            .db
            .prepare("SELECT key,value FROM settings WHERE key LIKE ?1 ORDER BY rowid")?;
        let rows = query
            .query_map([format!("{prefix}%")], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut commands = Vec::new();
        for (key, value) in rows {
            let command: BrowserCommand = serde_json::from_str(&value)?;
            if crate::store::now() - command.created > 120_000 {
                if let Some(mut record) = self.browser_record(&command.transfer_id)? {
                    record.control_error = true;
                    self.save_browser_record(&record)?;
                }
                self.db
                    .execute("DELETE FROM settings WHERE key=?1", [key])?;
            } else if commands.len() < 32 {
                commands.push(command);
            }
        }
        tx.commit()?;
        Ok(commands)
    }

    pub fn browser_control(&self, id: &str, action: &str) -> Result<JobView> {
        let Some(mut record) = self.browser_record(id)? else {
            bail!("job_not_found");
        };
        if !["pause", "resume", "cancel", "show"].contains(&action) {
            bail!("invalid_control_action");
        }
        if action == "resume" && !record.report.can_resume {
            bail!("browser_cannot_resume");
        }
        if action != "show" && ["completed", "canceled"].contains(&record.report.state.as_str()) {
            bail!("job_not_interruptible");
        }
        let command = BrowserCommand {
            id: uuid::Uuid::new_v4().to_string(),
            transfer_id: id.into(),
            action: action.into(),
            created: crate::store::now(),
        };
        let count: usize = self.db.query_row(
            "SELECT count(*) FROM settings WHERE key LIKE ?1",
            [format!("{COMMAND}{}:%", record.client)],
            |r| r.get(0),
        )?;
        if count >= 128 {
            bail!("browser_command_capacity");
        }
        let tx = self.db.unchecked_transaction()?;
        self.db.execute(
            "INSERT INTO settings VALUES(?1,?2)",
            params![
                format!("{COMMAND}{}:{}", record.client, command.id),
                serde_json::to_string(&command)?
            ],
        )?;
        record.control_error = false;
        self.save_browser_record(&record)?;
        tx.commit()?;
        self.browser_views()?
            .into_iter()
            .find(|job| job.id == id)
            .ok_or_else(|| anyhow::anyhow!("job_not_found"))
    }

    pub fn browser_needs_interface(&self, client: &str) -> Result<bool> {
        Ok(self.browser_records()?.iter().any(|r| {
            r.client == client
                && !r.shown
                && !["canceled", "failed"].contains(&r.report.state.as_str())
        }))
    }

    pub fn desktop_jobs(&self) -> Result<Vec<JobView>> {
        let mut jobs = Vec::new();
        for job in self.list()? {
            let phase: Option<String> = self.db.query_row("SELECT c.phase FROM capture_handoffs c JOIN handoffs h USING(request_id) WHERE h.job_id=?1", [&job.id], |r|r.get(0)).optional()?;
            if phase.is_none_or(|phase| phase == "committed") {
                jobs.push(job);
            }
        }
        jobs.extend(self.browser_views()?);
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_ms));
        jobs.truncate(1000);
        Ok(jobs)
    }

    pub fn browser_interface_requested(&self, client: &str) -> Result<()> {
        for mut record in self
            .browser_records()?
            .into_iter()
            .filter(|r| r.client == client && !r.shown)
        {
            record.shown = true;
            self.save_browser_record(&record)?;
        }
        Ok(())
    }
}
