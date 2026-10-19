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
            .desktop_jobs()
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
        if store.is_browser_transfer(&id).map_err(|e| e.to_string())? {
            return store
                .browser_control(&id, "resume")
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
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
        if job.policy.adaptive {
            command.arg("--adaptive");
        }
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
        let store = open()?;
        if store.is_browser_transfer(&id).map_err(|e| e.to_string())? {
            return store
                .browser_control(
                    &id,
                    match action.as_str() {
                        "paused" => "pause",
                        "queued" => "resume",
                        "canceled" => "cancel",
                        _ => return Err("invalid_control_action".into()),
                    },
                )
                .map_err(|e| e.to_string());
        }
        store.request(&id, &action).map_err(|e| e.to_string())
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
        store
            .set_queue_running(true)
            .map_err(|_| "operation_failed")?;
        idmxl_engine::application::ensure_queue_worker(&store, &engine_binary()?)
            .map_err(|e| e.to_string())
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
        let store = open()?;
        if store.is_browser_transfer(&id).map_err(|e| e.to_string())? {
            return store
                .browser_control(&id, "show")
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
        let job = store.get(&id).map_err(|_| "job_not_found")?;
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

#[tauri::command]
async fn check_updates() -> Result<idmxl_engine::updates::UpdateView, String> {
    tauri::async_runtime::spawn_blocking(|| {
        idmxl_engine::updates::check(&directory()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}
#[tauri::command]
async fn install_update(app: tauri::AppHandle, version: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let config = idmxl_engine::updates::configuration()
            .map_err(|e| e.to_string())?
            .ok_or("updates_not_configured")?;
        let profile = directory();
        {
            let store = open()?;
            store
                .set_queue_running(false)
                .map_err(|_| "operation_failed")?;
            for job in store.list().map_err(|_| "history_unavailable")? {
                if matches!(job.state.as_str(), "downloading" | "verifying" | "pausing") {
                    let _ = store.request(&job.id, "paused");
                }
            }
        }
        let installer =
            idmxl_engine::updates::download(&profile, &version).map_err(|e| e.to_string())?;
        let installed = std::env::current_exe()
            .map_err(|_| "operation_failed")?
            .parent()
            .ok_or("operation_failed")?
            .to_path_buf();
        let started = std::time::Instant::now();
        let recovery = loop {
            match idmxl_engine::updates::snapshot(&profile, &installed) {
                Ok(path) => break path,
                Err(e)
                    if e.to_string() == "update_requires_idle_app"
                        && started.elapsed().as_secs() < 10 =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(100))
                }
                Err(e) => return Err(e.to_string()),
            }
        };
        let file = std::fs::File::open(&installer).map_err(|_| "operation_failed")?;
        let hash = idmxl_engine::platform::hash_extent(
            &file,
            0,
            file.metadata().map_err(|_| "operation_failed")?.len(),
        )
        .map_err(|_| "operation_failed")?;
        drop(file);
        let script = recovery.join("install-update.ps1");
        std::fs::write(
            &script,
            include_bytes!("../../../../scripts/install-update.ps1"),
        )
        .map_err(|_| "operation_failed")?;
        let system = PathBuf::from(std::env::var_os("SystemRoot").ok_or("operation_failed")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = Command::new(system);
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
            .arg(script)
            .arg("-Installer")
            .arg(installer)
            .arg("-InstalledDirectory")
            .arg(installed)
            .arg("-RecoveryDirectory")
            .arg(recovery)
            .arg("-Sha256")
            .arg(hash)
            .arg("-Thumbprint")
            .arg(config.certificate_thumbprint)
            .arg("-ParentProcessId")
            .arg(std::process::id().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command.spawn().map_err(|_| "update_launch_failed")?;
        app.exit(0);
        Ok(())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn inspect_media(url: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        idmxl_engine::manifests::discover(&url, false)
            .and_then(|d| Ok(serde_json::to_value(d.view())?))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}
#[tauri::command]
async fn add_media(request: idmxl_engine::media::MediaRequest) -> Result<JobView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?.add_media(request, false).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}
#[tauri::command]
async fn media_details(id: String) -> Result<Option<serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?.media_details(&id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
async fn refresh_source(id: String, url: String) -> Result<JobView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?.refresh_source(&id, &url).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}
#[tauri::command]
fn quit_interface(app: tauri::AppHandle) {
    app.exit(0);
}
#[tauri::command]
fn completion_settings(enabled: Option<bool>) -> Result<serde_json::Value, String> {
    let store = open()?;
    if let Some(value) = enabled {
        store
            .set_completion_notifications(value)
            .map_err(|_| "operation_failed")?;
    }
    Ok(
        serde_json::json!({"notify":store.completion_notifications().map_err(|_|"operation_failed")?}),
    )
}
fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn watch_application_events(app: tauri::AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    std::thread::spawn(move || {
        let path = directory();
        let Ok(lock) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.join("completion.lock"))
        else {
            return;
        };
        if fs2::FileExt::try_lock_exclusive(&lock).is_err() {
            return;
        }
        let mut shown = open()
            .and_then(|s| s.interface_show_request().map_err(|e| e.to_string()))
            .ok()
            .flatten();
        loop {
            if let Ok(store) = open() {
                if let Ok(request) = store.interface_show_request()
                    && request != shown
                {
                    shown = request;
                    show_main(&app);
                }
                // Recover a worker that exited while a newer capture was enabling
                // the queue. This does not change the saved pause/run preference.
                if store.queue_running().unwrap_or(false)
                    && let Ok(executable) = engine_binary()
                {
                    let _ = idmxl_engine::application::ensure_queue_worker(&store, &executable);
                }
            }
            if let Ok(store) = open()
                && let Ok(jobs) = store.pending_completions()
            {
                let notify = store.completion_notifications().unwrap_or(false);
                for job in jobs {
                    if notify {
                        // Fixed text keeps download names and source secrets out of lock-screen notifications.
                        let _ = app
                            .notification()
                            .builder()
                            .title("Download complete")
                            .body("Your file is ready in IDMXL.")
                            .show();
                    }
                    let _ = store.acknowledge_completion(&job.id);
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

fn main() {
    let Ok(store) = open() else {
        return;
    };
    let Ok(Some(interface_lease)) = idmxl_engine::application::claim_interface(&store) else {
        return;
    };
    drop(store);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            app.manage(interface_lease);
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
                        show_main(app);
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
            let _ = open();
            watch_application_events(app.handle().clone());
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
            save_queue_window,
            inspect_media,
            add_media,
            media_details,
            check_updates,
            install_update,
            refresh_source,
            completion_settings,
            quit_interface
        ])
        .run(tauri::generate_context!())
        .expect("IDMXL desktop runtime failed");
}
