#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use idmxl_engine::{AddJob, Engine, JobView, Store};

fn directory() -> PathBuf {
    std::env::var_os("IDMXL_STATE_DIR")
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|p| PathBuf::from(p).join("IDMXL").into_os_string())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/IDMXL"))
}

fn open() -> Result<Store, String> {
    Store::open(&directory()).map_err(|_| "history_unavailable".into())
}

#[tauri::command]
async fn list_jobs() -> Result<Vec<JobView>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let engine = Engine::new(open()?);
        engine
            .reconcile()
            .map_err(|_| "history_unavailable".to_owned())?;
        engine
            .store
            .list()
            .map_err(|_| "history_unavailable".into())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn add_job(
    url: String,
    destination: String,
    replayable: bool,
    expected_sha256: Option<String>,
) -> Result<JobView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?
            .add(AddJob {
                url,
                destination: destination.into(),
                replayable,
                expected_sha256,
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn start_job(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = open()?;
        let job = store.view(&id).map_err(|_| "job_not_found".to_owned())?;
        if matches!(job.state.as_str(), "completed" | "canceled") {
            return Err("job_not_runnable".into());
        }
        let engine = Engine::new(store);
        engine
            .reconcile()
            .map_err(|_| "history_unavailable".to_owned())?;
        if engine
            .store
            .list()
            .map_err(|_| "history_unavailable".to_owned())?
            .iter()
            .any(|j| matches!(j.state.as_str(), "downloading" | "pausing" | "verifying"))
        {
            return Err("engine_busy".into());
        }
        #[cfg(debug_assertions)]
        let executable =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/idmxl.exe");
        #[cfg(not(debug_assertions))]
        let executable = std::env::current_exe()
            .map_err(|_| "engine_binary_missing".to_owned())?
            .parent()
            .ok_or("engine_binary_missing")?
            .join("idmxl.exe");
        if !executable.is_file() {
            return Err("engine_binary_missing".into());
        }
        let mut command = Command::new(executable);
        command
            .arg("--state-dir")
            .arg(directory())
            .arg("run")
            .arg(id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        // Child::drop does not terminate the CLI: downloads continue after the window closes.
        let child = command
            .spawn()
            .map_err(|_| "engine_launch_failed".to_owned())?;
        // Reap its process handle in a detached thread. The engine owns all transfer state.
        std::thread::spawn(move || {
            let mut child = child;
            let _ = child.wait();
        });
        Ok(())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn control_job(id: String, action: String) -> Result<JobView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?.request(&id, &action).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
fn capabilities() -> serde_json::Value {
    idmxl_engine::capabilities()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_jobs,
            add_job,
            start_job,
            control_job,
            capabilities
        ])
        .run(tauri::generate_context!())
        .expect("IDMXL desktop runtime failed");
}
