//! Explicit user choices for destinations, retries and file removal.
use crate::{JobView, Store, platform};
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};

#[derive(Default, Serialize, Deserialize)]
struct Locations {
    default: Option<PathBuf>,
    uses: std::collections::BTreeMap<PathBuf, u32>,
}

impl Store {
    pub fn manage_downloads(
        &self,
        ids: &[String],
        action: &str,
        confirmed: bool,
    ) -> Result<Vec<serde_json::Value>> {
        if !confirmed {
            bail!("confirmation_required");
        }
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        if ids.is_empty()
            || ids.len() > 1000
            || unique.len() != ids.len()
            || !["pause", "cancel", "delete"].contains(&action)
        {
            bail!("invalid_control_action");
        }
        // Validate the whole snapshot before stopping admission or changing a job.
        for id in ids {
            uuid::Uuid::parse_str(id)?;
            if !self.is_browser_transfer(id)? {
                self.get(id)?;
            }
        }
        if ids.len() > 1 || action == "pause" {
            self.set_queue_running(false)?;
        }
        if action == "delete" {
            for id in ids {
                if !self.is_browser_transfer(id)?
                    && let Ok(job) = self.view(id)
                    && !["completed", "canceled", "publishing"].contains(&job.state.as_str())
                {
                    let _ = self.request(id, "canceled");
                }
            }
            let started = std::time::Instant::now();
            while started.elapsed() < std::time::Duration::from_secs(10) {
                if !ids.iter().any(|id| {
                    self.view(id).is_ok_and(|j| {
                        ["downloading", "pausing", "verifying"].contains(&j.state.as_str())
                    })
                }) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        Ok(ids.iter().map(|id| {
            let result=(||->Result<()>{
                if self.is_browser_transfer(id)? {self.browser_control(id,action)?;}
                else if action=="delete" {self.delete_download_file(id,true)?;}
                else {self.request(id,if action=="pause"{"paused"}else{"canceled"})?;}
                Ok(())
            })();
            serde_json::json!({"id":id,"success":result.is_ok(),"error":result.err().map(|e|e.to_string())})
        }).collect())
    }
    pub fn location_declined(&self, request_id: &str) -> Result<bool> {
        self.setting_bool(&format!("location_declined:{request_id}"), false)
    }
    pub fn decline_location(&self, request_id: &str) -> Result<()> {
        uuid::Uuid::parse_str(request_id).context("invalid_request_id")?;
        self.db.execute(
            "INSERT INTO settings VALUES(?1,'true') ON CONFLICT(key) DO NOTHING",
            [format!("location_declined:{request_id}")],
        )?;
        Ok(())
    }
    fn locations(&self) -> Result<Locations> {
        let value: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM settings WHERE key='download_locations'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(value
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default())
    }
    fn save_locations(&self, value: &Locations) -> Result<()> {
        self.db.execute("INSERT INTO settings VALUES('download_locations',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn suggested_directory(&self, fallback: &Path) -> Result<PathBuf> {
        Ok(self
            .locations()?
            .default
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| fallback.to_owned()))
    }
    pub fn set_download_directory(&self, directory: &Path) -> Result<PathBuf> {
        let directory = directory
            .canonicalize()
            .context("destination_folder_missing")?;
        if !directory.is_dir() || platform::is_reparse(&directory)? {
            bail!("invalid_destination");
        }
        let tx = self.write_transaction()?;
        let mut locations = self.locations()?;
        locations.default = Some(directory.clone());
        self.save_locations(&locations)?;
        tx.commit()?;
        Ok(directory)
    }
    /// One successful confirmation is counted once, even after a lost host reply.
    pub fn remember_destination(&self, confirmation: &str, destination: &Path) -> Result<()> {
        uuid::Uuid::parse_str(confirmation).context("invalid_request_id")?;
        let parent = destination
            .parent()
            .context("invalid_destination")?
            .canonicalize()?;
        let tx = self.write_transaction()?;
        let key = format!("location_confirmation:{confirmation}");
        if self
            .db
            .execute("INSERT OR IGNORE INTO settings VALUES(?1,'true')", [key])?
            != 0
        {
            let mut locations = self.locations()?;
            let count = locations.uses.entry(parent.clone()).or_default();
            *count = count.saturating_add(1);
            if *count > 3 {
                locations.default = Some(parent);
            }
            self.save_locations(&locations)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn retry_now(&self, id: &str) -> Result<JobView> {
        let tx = self.write_transaction()?;
        let job = self.get(id)?;
        if job.state != "failed" {
            bail!("retry_requires_failed_job");
        }
        self.capture_runnable(id)?;
        self.reset_retry_budget(id)?;
        // Override the saved wait once for this job and its media components.
        // Other jobs on the same server retain their existing cooldown.
        self.db.execute(
            "INSERT INTO settings VALUES(?1,'true') ON CONFLICT(key) DO UPDATE SET value='true'",
            [format!("retry_override:{id}")],
        )?;
        self.db.execute("INSERT INTO settings SELECT 'retry_override:'||job_id,'true' FROM media_components WHERE parent_id=?1 ON CONFLICT(key) DO UPDATE SET value='true'",[id])?;
        self.db.execute(
            "UPDATE jobs SET state='queued',requested='run',error=NULL WHERE id=?1",
            [id],
        )?;
        tx.commit()?;
        self.view(id)
    }
    pub(crate) fn consume_retry_override(&self, id: &str) -> Result<bool> {
        Ok(self.db.execute(
            "DELETE FROM settings WHERE key=?1",
            [format!("retry_override:{id}")],
        )? > 0)
    }
    /// Change the final destination, copying any staging data before changing
    /// the database so relocation to another volume preserves checkpoints.
    pub fn change_destination(&self, id: &str, path: &Path) -> Result<JobView> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join("engine.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock).context("engine_busy")?;
        let tx = self.write_transaction()?;
        let job = self.get(id)?;
        if !["queued", "paused", "failed", "canceled"].contains(&job.state.as_str()) {
            bail!("destination_change_requires_pause");
        }
        if self.is_media(id)? {
            bail!("media_destination_change_unsupported");
        }
        let destination = platform::destination(path)?;
        if self.is_media(id)? && destination.extension() != job.destination.extension() {
            bail!("media_extension_mismatch");
        }
        let staging = destination
            .parent()
            .context("invalid_destination")?
            .join(format!(".idmxl-{id}.part"));
        let mut copied = None;
        if staging != job.staging && job.staging.try_exists()? {
            if platform::is_reparse(&job.staging)? {
                bail!("file_identity_changed");
            }
            let mut source = std::fs::File::open(&job.staging)?;
            let mut target = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging)
                .context("destination_in_use")?;
            let copied_result = (|| -> Result<()> {
                std::io::copy(&mut source, &mut target)?;
                target.sync_all()?;
                Ok(())
            })();
            let identity = platform::file_id(&target)?;
            drop(target);
            if let Err(error) = copied_result {
                let _ = platform::delete_owned_file(&staging, Some(&identity));
                return Err(error);
            }
            copied = Some(identity);
        }
        let updated = self
            .db
            .execute(
                "UPDATE jobs SET destination=?1,staging=?2 WHERE id=?3",
                params![
                    destination.to_str().context("non_unicode_path")?,
                    staging.to_str().context("non_unicode_path")?,
                    id
                ],
            )
            .context("destination_in_use");
        if let Err(error) = updated {
            if let Some(identity) = copied {
                let _ = platform::delete_owned_file(&staging, Some(&identity));
            }
            return Err(error);
        }
        tx.commit()?;
        if copied.is_some() {
            let _ = platform::delete_owned_file(&job.staging, None);
        }
        self.remember_destination(&uuid::Uuid::new_v4().to_string(), &destination)?;
        self.view(id)
    }
    /// Delete only the recorded file, never a directory or arbitrary supplied path.
    /// Completed files must still have the publication identity recorded by IDMXL.
    pub fn delete_download_file(&self, id: &str, confirmed: bool) -> Result<JobView> {
        if !confirmed {
            bail!("confirmation_required");
        }
        let job = self.get(id)?;
        if [
            "downloading",
            "pausing",
            "verifying",
            "publishing",
            "awaiting_browser",
            "preparing_media",
        ]
        .contains(&job.state.as_str())
        {
            bail!("delete_requires_stopped_job");
        }
        let _lock = if job.state != "completed" {
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(self.directory.join("engine.lock"))?;
            fs2::FileExt::try_lock_exclusive(&lock).context("engine_busy")?;
            if job.state != "canceled" {
                self.request(id, "canceled")?;
            }
            Some(lock)
        } else {
            None
        };
        let path = if job.state == "completed" {
            &job.destination
        } else {
            &job.staging
        };
        if !path.try_exists()? {
            return self.view(id);
        }
        if platform::is_reparse(path)? || !path.is_file() {
            bail!("file_identity_changed");
        }
        let expected = if job.state == "completed" {
            Some(
                job.publication_file_id
                    .as_deref()
                    .context("file_identity_changed")?,
            )
        } else {
            if job.publication_file_id.is_none()
                && job.staging.file_name()
                    != Some(std::ffi::OsStr::new(&format!(".idmxl-{id}.part")))
            {
                bail!("file_identity_changed");
            }
            job.publication_file_id.as_deref()
        };
        platform::delete_owned_file(path, expected)?;
        self.db.execute(
            "INSERT INTO settings VALUES(?1,'true') ON CONFLICT(key) DO UPDATE SET value='true'",
            [format!("file_deleted:{id}")],
        )?;
        self.view(id)
    }
}

/// A native Save As dialog. It runs before a browser handoff can commit or start.
#[cfg(windows)]
pub fn choose_download_path(default: &Path) -> Result<Option<PathBuf>> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetSaveFileNameW, OFN_EXPLORER, OFN_HIDEREADONLY, OFN_NOCHANGEDIR,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    // The legacy Windows dialog does not accept the extended-length prefix
    // returned by canonicalize(), even though filesystem operations do.
    let text = default.to_string_lossy();
    let normal = if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
    };
    let initial: Vec<u16> = normal.encode_utf16().collect();
    if initial.len() >= 32767 {
        bail!("invalid_destination");
    }
    let mut buffer = vec![0u16; 32768];
    buffer[..initial.len()].copy_from_slice(&initial);
    let mut title: Vec<u16> = "IDMXL — Choose download location\0"
        .encode_utf16()
        .collect();
    let mut options: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    options.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    options.lpstrFile = buffer.as_mut_ptr();
    options.nMaxFile = buffer.len() as u32;
    options.lpstrTitle = title.as_ptr();
    options.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY;
    loop {
        options.lpstrTitle = title.as_ptr();
        if unsafe { GetSaveFileNameW(&mut options) } == 0 {
            let code = unsafe { CommDlgExtendedError() };
            if code != 0 {
                eprintln!("IDMXL location chooser failed (Windows dialog code {code:#x}).");
                bail!("location_dialog_failed");
            }
            return Ok(None);
        }
        let end = buffer
            .iter()
            .position(|&c| c == 0)
            .context("invalid_destination")?;
        let path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end]));
        match platform::destination(&path) {
            Ok(path) => return Ok(Some(path)),
            Err(error) if error.to_string() == "destination_exists" => {
                title = "IDMXL — File already exists. Choose another name\0"
                    .encode_utf16()
                    .collect();
            }
            Err(error) => return Err(error),
        }
    }
}
#[cfg(not(windows))]
pub fn choose_download_path(_: &Path) -> Result<Option<PathBuf>> {
    bail!("location_dialog_unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fourth_confirmation_sets_default_and_replays_do_not_count() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let folder = tempfile::tempdir().unwrap();
        let fallback = tempfile::tempdir().unwrap();
        let path = folder.path().join("file.zip");
        let id = uuid::Uuid::new_v4().to_string();
        for _ in 0..5 {
            store.remember_destination(&id, &path).unwrap();
        }
        assert_eq!(
            store.suggested_directory(fallback.path()).unwrap(),
            fallback.path()
        );
        for _ in 0..3 {
            store
                .remember_destination(&uuid::Uuid::new_v4().to_string(), &path)
                .unwrap();
        }
        assert_eq!(
            store.suggested_directory(fallback.path()).unwrap(),
            folder.path().canonicalize().unwrap()
        );
        store.set_download_directory(fallback.path()).unwrap();
        assert_eq!(
            store.suggested_directory(folder.path()).unwrap(),
            fallback.path().canonicalize().unwrap()
        );
    }
    #[test]
    fn relocation_preserves_partial_bytes_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let job = store
            .add(crate::AddJob {
                url: "https://example.org/a".into(),
                destination: root.path().join("a.zip"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        let staging = store.get(&job.id).unwrap().staging;
        std::fs::write(&staging, b"partial").unwrap();
        let dest = root.path().join("b.zip");
        store.change_destination(&job.id, &dest).unwrap();
        assert_eq!(
            std::fs::read(store.get(&job.id).unwrap().staging).unwrap(),
            b"partial"
        );
        std::fs::write(root.path().join("exists.zip"), b"keep").unwrap();
        assert!(
            store
                .change_destination(&job.id, &root.path().join("exists.zip"))
                .is_err()
        );
        assert_eq!(
            store.get(&job.id).unwrap().destination,
            dest.parent().unwrap().canonicalize().unwrap().join("b.zip")
        );
        assert!(store.delete_download_file(&job.id, false).is_err());
        assert!(store.get(&job.id).unwrap().staging.exists());
    }
    #[test]
    fn immediate_retry_is_explicit_and_clears_saved_wait_and_budget() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let job = store
            .add(crate::AddJob {
                url: "https://example.org/a".into(),
                destination: root.path().join("a"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        assert!(store.retry_now(&job.id).is_err());
        store
            .state(&job.id, "failed", Some("server_throttled"))
            .unwrap();
        store
            .defer_origin(
                "https://example.org/a",
                std::time::Duration::from_secs(3600),
            )
            .unwrap();
        assert!(store.view(&job.id).unwrap().retry_not_before.is_some());
        let retried = store.retry_now(&job.id).unwrap();
        assert_eq!(retried.state, "queued");
        assert!(retried.retry_not_before.is_none());
        assert!(
            store
                .retry_deadline("https://example.org/other")
                .unwrap()
                .is_some()
        );
        assert!(store.consume_retry_override(&job.id).unwrap());
        assert!(!store.consume_retry_override(&job.id).unwrap());
    }

    #[test]
    fn confirmed_deletion_removes_only_the_published_identity_and_keeps_history() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let job = store
            .add(crate::AddJob {
                url: "https://example.org/a".into(),
                destination: root.path().join("a.zip"),
                replayable: false,
                expected_sha256: None,
            })
            .unwrap();
        let path = store.get(&job.id).unwrap().destination;
        std::fs::write(&path, b"owned").unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let identity = platform::file_id(&file).unwrap();
        drop(file);
        store
            .publication_intent(&job.id, 5, "fixture", &identity)
            .unwrap();
        store.complete(&job.id).unwrap();
        assert!(store.delete_download_file(&job.id, false).is_err());
        assert!(path.exists());
        store.delete_download_file(&job.id, true).unwrap();
        assert!(!path.exists());
        assert!(!store.view(&job.id).unwrap().file_available);
        assert_eq!(store.list().unwrap().len(), 1);
        std::fs::write(&path, b"new file").unwrap();
        assert!(store.delete_download_file(&job.id, true).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"new file");
    }

    #[test]
    fn deletion_rejects_replaced_completed_files_and_live_partial_files() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let job = store
            .add(crate::AddJob {
                url: "https://example.org/a".into(),
                destination: root.path().join("a.zip"),
                replayable: false,
                expected_sha256: None,
            })
            .unwrap();
        let data = store.get(&job.id).unwrap();
        std::fs::write(&data.staging, b"partial").unwrap();
        store.state(&job.id, "downloading", None).unwrap();
        assert!(store.delete_download_file(&job.id, true).is_err());
        assert!(data.staging.exists());
        store.state(&job.id, "completed", None).unwrap();
        std::fs::write(&data.destination, b"unrelated").unwrap();
        assert!(store.delete_download_file(&job.id, true).is_err());
        assert!(data.destination.exists());
    }

    #[test]
    fn canceled_location_survives_reopen_and_never_creates_a_job() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let store = Store::open(root.path()).unwrap();
        store.decline_location(&id).unwrap();
        drop(store);
        let store = Store::open(root.path()).unwrap();
        assert!(store.location_declined(&id).unwrap());
        assert_eq!(
            store.capture_status(&id).unwrap_err().to_string(),
            "user_cancelled"
        );
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn bulk_confirmation_is_required_and_the_snapshot_is_validated_before_mutation() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let mut ids = Vec::new();
        for name in ["a", "b"] {
            let job = store
                .add(crate::AddJob {
                    url: format!("https://example.org/{name}"),
                    destination: root.path().join(name),
                    replayable: false,
                    expected_sha256: None,
                })
                .unwrap();
            std::fs::write(store.get(&job.id).unwrap().staging, b"partial").unwrap();
            ids.push(job.id);
        }
        store.set_queue_running(true).unwrap();
        for action in ["pause", "cancel", "delete"] {
            assert!(store.manage_downloads(&ids, action, false).is_err());
        }
        let mut invalid = ids.clone();
        invalid.push(uuid::Uuid::new_v4().to_string());
        assert!(store.manage_downloads(&invalid, "delete", true).is_err());
        assert!(store.queue_running().unwrap());
        assert!(
            ids.iter()
                .all(|id| store.get(id).unwrap().state == "queued")
        );
        assert!(
            store
                .manage_downloads(&ids, "pause", true)
                .unwrap()
                .iter()
                .all(|r| r["success"] == true)
        );
        assert!(!store.queue_running().unwrap());
        assert!(
            ids.iter()
                .all(|id| store.get(id).unwrap().state == "paused")
        );
        assert!(
            store
                .manage_downloads(&ids, "cancel", true)
                .unwrap()
                .iter()
                .all(|r| r["success"] == true)
        );
        assert!(ids.iter().all(|id| store.get(id).unwrap().staging.exists()));
        assert!(
            store
                .manage_downloads(&ids, "delete", true)
                .unwrap()
                .iter()
                .all(|r| r["success"] == true)
        );
        assert!(
            ids.iter()
                .all(|id| !store.get(id).unwrap().staging.exists())
        );
        assert_eq!(store.list().unwrap().len(), 2);
    }
}
