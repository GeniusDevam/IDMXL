use super::{engine_binary, open};
use idmxl_engine::JobView;

#[tauri::command]
pub async fn download_location(directory: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = open()?;
        let path = if let Some(path) = directory {
            store.set_download_directory(std::path::Path::new(&path))
        } else {
            let fallback = std::env::var_os("USERPROFILE")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(super::directory)
                .join("Downloads");
            store.suggested_directory(&fallback)
        }
        .map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
pub async fn change_destination(id: String, destination: String) -> Result<JobView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?
            .change_destination(&id, std::path::Path::new(&destination))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

#[tauri::command]
pub async fn retry_job(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        idmxl_engine::application::control_download(&open()?, &engine_binary()?, &id, "retry")
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}

/// A fixed snapshot is supplied by the confirmation dialog. New arrivals are
/// not silently included in an already-confirmed destructive selection.
#[tauri::command]
pub async fn manage_downloads(
    ids: Vec<String>,
    action: String,
    confirmed: bool,
) -> Result<Vec<serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        open()?
            .manage_downloads(&ids, &action, confirmed)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "operation_failed".to_owned())?
}
