#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use idmxl_engine::{AddJob, Engine, JobPolicy, JobView, Store};
use tauri::Manager;

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
            .arg("--connections")
            .arg(job.policy.connections.to_string())
            .arg("--retries")
            .arg(job.policy.retries.to_string())
            .arg("--speed-limit")
            .arg(job.policy.speed_limit.to_string())
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
async fn set_policy(id: String, policy: JobPolicy) -> Result<JobView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?.set_policy(&id, &policy).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

fn engine_binary() -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    let executable =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/idmxl.exe");
    #[cfg(not(debug_assertions))]
    let executable = std::env::current_exe()
        .map_err(|_| "engine_binary_missing")?
        .parent()
        .ok_or("engine_binary_missing")?
        .join("idmxl.exe");
    if !executable.is_file() {
        return Err("engine_binary_missing".into());
    }
    Ok(executable)
}

#[tauri::command]
async fn workflow_settings() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let store = open()?;
        Ok(
            serde_json::json!({"folders":store.folder_settings().map_err(|_|"operation_failed")?,
            "default_rules":idmxl_engine::workflows::default_folder_rules()}),
        )
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn save_folder_settings(
    settings: idmxl_engine::FolderSettings,
) -> Result<idmxl_engine::FolderSettings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?
            .set_folder_settings(&settings)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn add_links(batch: idmxl_engine::LinkBatch) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(serde_json::json!({"results":open()?.add_links(batch).map_err(|e|e.to_string())?}))
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn get_queue_window(queue_name: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = open()?;
        Ok(
            serde_json::json!({"window":store.queue_window(&queue_name).map_err(|e|e.to_string())?,
            "open_now":store.queue_open(&queue_name).map_err(|e|e.to_string())?}),
        )
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn save_queue_window(
    queue_name: String,
    window: Option<idmxl_engine::QueueWindow>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?
            .set_queue_window(&queue_name, window.as_ref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn queue_control(running: bool, speed_limit: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = open()?;
        if let Some(limit) = speed_limit {
            store
                .set_global_speed_limit(limit.parse().map_err(|_| "invalid_speed_limit")?)
                .map_err(|_| "invalid_speed_limit")?;
        }
        if !running {
            return store
                .set_queue_running(false)
                .map_err(|_| "operation_failed".into());
        }
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory().join("scheduler.lock"))
            .map_err(|_| "operation_failed")?;
        if fs2::FileExt::try_lock_exclusive(&lock).is_err() {
            return store
                .set_queue_running(true)
                .map_err(|_| "operation_failed".into());
        }
        let mut command = Command::new(engine_binary()?);
        command
            .arg("--state-dir")
            .arg(directory())
            .args(["queue-run", "--watch"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        drop(lock);
        let mut child = command.spawn().map_err(|_| "engine_launch_failed")?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn queue_status() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let store = open()?;
        let lock = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true)
            .open(directory().join("scheduler.lock")).map_err(|_|"operation_failed")?;
        let active = fs2::FileExt::try_lock_exclusive(&lock).is_err();
        Ok(serde_json::json!({"running":active && store.queue_running().map_err(|_|"operation_failed")?,
            "speed_limit":store.global_speed_limit().map_err(|_|"operation_failed")?.to_string()}))
    }).await.map_err(|_|"operation_failed".to_owned())?
}

#[tauri::command]
async fn reveal_job(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let job = open()?.get(&id).map_err(|_| "job_not_found")?;
        if job.state != "completed" || !job.destination.is_file() {
            return Err("file_unavailable".into());
        }
        let path = job.destination.to_string_lossy();
        let path = if let Some(unc) = path.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{unc}")
        } else {
            path.strip_prefix("\\\\?\\").unwrap_or(&path).to_owned()
        };
        Command::new("explorer.exe")
            .arg("/select,")
            .arg(path)
            .spawn()
            .map_err(|_| "reveal_failed")?;
        Ok(())
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
        .setup(|app| {
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };
            let show = MenuItem::with_id(app, "show", "Show IDMXL", true, None::<&str>)?;
            let pause =
                MenuItem::with_id(app, "pause", "Pause download queue", true, None::<&str>)?;
            let exit = MenuItem::with_id(
                app,
                "exit",
                "Exit interface (keep downloads running)",
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&show, &pause, &exit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("IDMXL — background downloads")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "pause" => {
                        if let Ok(store) = open() {
                            let _ = store.set_queue_running(false);
                        }
                    }
                    "exit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.hide().is_ok()
            {
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_jobs,
            add_job,
            start_job,
            control_job,
            set_policy,
            queue_control,
            queue_status,
            reveal_job,
            capabilities,
            workflow_settings,
            save_folder_settings,
            add_links,
            get_queue_window,
            save_queue_window
        ])
        .run(tauri::generate_context!())
        .expect("IDMXL desktop runtime failed");
}
