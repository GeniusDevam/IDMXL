//! Local application activation. Browser input never chooses executables or arguments.
use crate::Store;
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn lock(store: &Store, name: &str) -> Result<File> {
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.directory.join(name))?)
}

fn contended(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock
        || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
}

impl Store {
    pub fn request_interface_show(&self) -> Result<()> {
        self.db.execute("INSERT INTO settings VALUES('interface_show',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [uuid::Uuid::new_v4().to_string()])?;
        Ok(())
    }

    pub fn interface_show_request(&self) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row(
                "SELECT value FROM settings WHERE key='interface_show'",
                [],
                |r| r.get(0),
            )
            .optional()?)
    }
}

/// Keep this lease alive for the interface's lifetime. A second launch asks the
/// existing instance to show itself instead of creating another tray/window.
pub fn claim_interface(store: &Store) -> Result<Option<File>> {
    let lease = lock(store, "interface.lock")?;
    match fs2::FileExt::try_lock_exclusive(&lease) {
        Ok(()) => Ok(Some(lease)),
        Err(error) if contended(&error) => {
            store.request_interface_show()?;
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

fn background(command: &mut Command) -> std::io::Result<std::process::Child> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::Foundation::{
            HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
        };
        // Native-messaging pipes can be inheritable even though the child uses
        // NUL for its own stdio. Do not let a long-lived worker keep those pipes
        // open after the host exits (which would stall Chrome's acknowledgement).
        for handle in [
            std::io::stdin().as_raw_handle(),
            std::io::stdout().as_raw_handle(),
            std::io::stderr().as_raw_handle(),
        ] {
            if !handle.is_null()
                && handle != INVALID_HANDLE_VALUE
                && unsafe { SetHandleInformation(handle.cast(), HANDLE_FLAG_INHERIT, 0) } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
        }
        // Leave a browser/native-host job when permitted. Some launchers do not
        // permit breakaway; retry the ordinary hidden launch only in that case.
        command.creation_flags(0x08000000 | 0x01000000);
        match command.spawn() {
            Err(error) if error.raw_os_error() == Some(5) => {
                command.creation_flags(0x08000000);
                command.spawn()
            }
            result => result,
        }
    }
    #[cfg(not(windows))]
    command.spawn()
}

/// Start one persistent worker per profile. Paused/failed jobs remain excluded,
/// and the scheduler still applies queue hours, deadlines and speed limits.
pub fn ensure_queue_worker(store: &Store, executable: &Path) -> Result<()> {
    let startup = lock(store, "scheduler-start.lock")?;
    fs2::FileExt::lock_exclusive(&startup)?;
    if !store.queue_running()? {
        bail!("queue_paused");
    }
    let scheduler = lock(store, "scheduler.lock")?;
    match fs2::FileExt::try_lock_exclusive(&scheduler) {
        Err(error) if contended(&error) => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(()) => drop(scheduler),
    }
    let mut command = Command::new(executable);
    let startup_id = uuid::Uuid::new_v4();
    let ready_path = store.directory.join(format!(".worker-ready-{startup_id}"));
    command
        .arg("--state-dir")
        .arg(&store.directory)
        .args(["queue-run", "--watch", "--startup-id"])
        .arg(startup_id.to_string());
    let mut child = background(&mut command).context("engine_launch_failed")?;
    let started = Instant::now();
    loop {
        if ready_path.is_file() {
            std::fs::remove_file(&ready_path)?;
            break;
        }
        if child.try_wait()?.is_some() {
            bail!("engine_launch_failed");
        }
        if started.elapsed() > Duration::from_secs(5) {
            bail!("engine_start_timeout");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct BrowserActivation {
    pub queue_started: bool,
    pub desktop_requested: bool,
}

pub fn open_interface(store: &Store, executable: &Path) -> Result<bool> {
    let desktop = executable
        .parent()
        .context("desktop_binary_missing")?
        .join(if cfg!(windows) {
            "idmxl-desktop.exe"
        } else {
            "idmxl-desktop"
        });
    if !desktop.is_file() {
        return Ok(false);
    }
    let mut command = Command::new(desktop);
    command.env("IDMXL_STATE_DIR", &store.directory);
    let mut child = background(&mut command).context("desktop_launch_failed")?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(true)
}

/// Retry missing launch effects after a lost acknowledgement. Successful launch
/// effects are journaled so duplicate commits cannot undo a later manual pause.
pub fn activate_capture(
    store: &Store,
    request_id: &str,
    executable: &Path,
) -> Result<BrowserActivation> {
    let activation_lock = lock(store, "browser-activation.lock")?;
    fs2::FileExt::lock_exclusive(&activation_lock)?;
    if store.capture_status(request_id)?.phase != "committed" {
        bail!("handoff_not_ready");
    }
    let key = format!("capture_activation:{request_id}");
    let saved: Option<String> = store
        .db
        .query_row("SELECT value FROM settings WHERE key=?1", [&key], |r| {
            r.get(0)
        })
        .optional()?;
    let mut activation: BrowserActivation = saved
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    if !activation.queue_started && store.queue_running()? {
        activation.queue_started = ensure_queue_worker(store, executable).is_ok();
    }
    if !activation.desktop_requested {
        activation.desktop_requested = open_interface(store, executable).unwrap_or(false);
    }
    store.db.execute(
        "INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, serde_json::to_string(&activation)?],
    )?;
    Ok(activation)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interface_lease_reuses_one_window_and_is_scoped_to_the_profile() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let lease = claim_interface(&store).unwrap().unwrap();
        assert!(store.interface_show_request().unwrap().is_none());
        assert!(claim_interface(&store).unwrap().is_none());
        let first = store.interface_show_request().unwrap();
        assert!(first.is_some());
        assert!(claim_interface(&store).unwrap().is_none());
        assert_ne!(store.interface_show_request().unwrap(), first);
        let other = tempfile::tempdir().unwrap();
        assert!(
            claim_interface(&Store::open(other.path()).unwrap())
                .unwrap()
                .is_some()
        );
        drop(lease);
        assert!(claim_interface(&store).unwrap().is_some());
    }
}
