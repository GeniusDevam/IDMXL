//! A fixed bundled worker, never an executable or command line chosen by a website.
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
}

fn exchange(directory: &Path, request: &Value) -> Result<Value> {
    let bytes =
        std::fs::read(directory.join("torrent/worker.json")).context("torrent_worker_offline")?;
    if bytes.len() > 1024 {
        bail!("invalid_torrent_endpoint");
    }
    let endpoint: Endpoint = serde_json::from_slice(&bytes)?;
    if endpoint.token.len() != 64 {
        bail!("invalid_torrent_endpoint");
    }
    let address: SocketAddr = ([127, 0, 0, 1], endpoint.port).into();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))
        .context("torrent_worker_offline")?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut request = request
        .as_object()
        .context("invalid_torrent_message")?
        .clone();
    request.insert("token".into(), endpoint.token.into());
    let bytes = serde_json::to_vec(&request)?;
    if bytes.len() > 2 * 1024 * 1024 {
        bail!("invalid_torrent_message");
    }
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(&bytes)?;
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    if size > 2 * 1024 * 1024 {
        bail!("invalid_torrent_response");
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    let response: Value = serde_json::from_slice(&bytes)?;
    if response["ok"] != true {
        bail!(
            "{}",
            response["error"]
                .as_str()
                .unwrap_or("torrent_operation_failed")
        );
    }
    Ok(response["result"].clone())
}

pub fn worker_binary(engine: &Path) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        let _ = engine;
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src-tauri/torrent/idmxl-torrent.exe")
    }
    #[cfg(not(debug_assertions))]
    {
        engine
            .parent()
            .unwrap_or(Path::new("."))
            .join("torrent/idmxl-torrent.exe")
    }
}

pub fn inspect_browser(
    directory: &Path,
    engine: &Path,
    source: &str,
    context: Option<&crate::request_context::RequestContext>,
) -> Result<Value> {
    if source.starts_with("magnet:?") {
        if source.len() > 16384 {
            bail!("invalid_torrent_source");
        }
        return request(
            directory,
            engine,
            &serde_json::json!({"kind":"inspect","source":source}),
        );
    }
    crate::platform::checked_url(source)?;
    if let Some(context) = context {
        context.validate(source)?;
    }
    let (_, bytes) = crate::small_http::fetch_context(source, 16 * 1024 * 1024, true, context)?;
    let inbox = directory.join("torrent/inbox");
    std::fs::create_dir_all(&inbox)?;
    let path = inbox.join(format!("{}.torrent", uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    let result = request(
        directory,
        engine,
        &serde_json::json!({"kind":"inspect","source":path}),
    );
    let _ = std::fs::remove_file(path);
    result
}

pub fn request(directory: &Path, engine: &Path, request: &Value) -> Result<Value> {
    match exchange(directory, request) {
        Ok(result) => return Ok(result),
        Err(error) if error.to_string() != "torrent_worker_offline" => return Err(error),
        _ => {}
    }
    if request["kind"] == "shutdown" {
        return Ok(serde_json::json!({"stopping":true}));
    }
    let binary = worker_binary(engine);
    if !binary.is_file() {
        bail!("torrent_worker_missing");
    }
    let mut command = Command::new(binary);
    command
        .arg("--state")
        .arg(directory.join("torrent"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().context("torrent_worker_start_failed")?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(150));
        match exchange(directory, request) {
            Ok(result) => return Ok(result),
            Err(error) if error.to_string() != "torrent_worker_offline" => return Err(error),
            _ => {}
        }
    }
    bail!("torrent_worker_start_failed")
}
