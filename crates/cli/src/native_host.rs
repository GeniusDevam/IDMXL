use anyhow::{Context, Result, bail};
use idmxl_engine::{AddJob, Store};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    path::PathBuf,
};

const MAX_FRAME: usize = 256 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    allowed_origin: String,
    state_dir: PathBuf,
    download_dir: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    kind: String,
    request_id: String,
    url: Option<String>,
    filename: Option<String>,
    etag: Option<String>,
    total: Option<String>,
    browser_cancelled: Option<bool>,
    selection: Option<String>,
    audio_selection: Option<String>,
    container: Option<String>,
    audio_only: Option<bool>,
}

fn safe_name(name: Option<&str>) -> String {
    let text: String = name
        .unwrap_or("download.bin")
        .chars()
        .take(100)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let text = text.trim_matches('.');
    if text.is_empty() {
        "download.bin".into()
    } else {
        text.into()
    }
}

fn read_frame(input: &mut impl Read) -> Result<Option<Vec<u8>>> {
    let mut size = [0u8; 4];
    match input.read(&mut size[..1])? {
        0 => return Ok(None),
        1 => {}
        _ => unreachable!(),
    }
    input
        .read_exact(&mut size[1..])
        .context("truncated_frame")?;
    let size = u32::from_ne_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME {
        bail!("invalid_frame_size");
    }
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes).context("truncated_frame")?;
    Ok(Some(bytes))
}

pub fn serve(origin: &str) -> Result<()> {
    let config_path = std::env::var_os("IDMXL_HOST_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                .join("IDMXL/browser-host.json")
        });
    let data = std::fs::read(config_path).context("host_not_configured")?;
    if data.len() > 16384 {
        bail!("invalid_host_configuration");
    }
    let config: Config = serde_json::from_slice(&data).context("invalid_host_configuration")?;
    if !origin.starts_with("chrome-extension://")
        || !origin.ends_with('/')
        || origin != config.allowed_origin
    {
        bail!("origin_not_allowed");
    }
    let directory = config
        .download_dir
        .canonicalize()
        .context("download_directory_missing")?;
    if !directory.is_dir() || idmxl_engine::platform::is_reparse(&directory)? {
        bail!("invalid_download_directory");
    }
    let store = Store::open(&config.state_dir)?;
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    while let Some(frame) = read_frame(&mut input)? {
        let result = (|| -> Result<serde_json::Value> {
            let request: Request = serde_json::from_slice(&frame).context("invalid_message")?;
            if !(request.version == 1 && request.kind == "enqueue"
                || request.version == 2
                    && matches!(
                        request.kind.as_str(),
                        "prepare" | "status" | "commit" | "abort"
                    )
                || request.version == 3
                    && matches!(request.kind.as_str(), "media_inspect" | "media_enqueue"))
            {
                bail!("unsupported_message");
            }
            let id = uuid::Uuid::parse_str(&request.request_id).context("invalid_request_id")?;
            if request.version == 3 {
                if request.etag.is_some()
                    || request.total.is_some()
                    || request.browser_cancelled.is_some()
                {
                    bail!("invalid_message");
                }
                let url = request.url.context("invalid_message")?;
                if request.kind == "media_inspect" {
                    let inspection = idmxl_engine::manifests::discover(&url, true)?.view();
                    return Ok(
                        serde_json::json!({"version":3,"accepted":true,"request_id":id.to_string(),"inspection":inspection}),
                    );
                }
                let container = request.container.context("invalid_message")?;
                if !["mp4", "mkv", "webm", "mov", "m4a", "mp3", "wav", "ogg"]
                    .contains(&container.as_str())
                {
                    bail!("invalid_message");
                }
                let name = format!(
                    "{id}-{}.{}",
                    safe_name(request.filename.as_deref()),
                    container
                );
                let job = store.add_media_handoff(
                    idmxl_engine::media::MediaRequest {
                        url,
                        selection: request.selection.context("invalid_message")?,
                        audio_selection: request.audio_selection,
                        audio_url: None,
                        destination: directory.join(name),
                        container,
                        audio_only: request.audio_only.unwrap_or(false),
                    },
                    &id.to_string(),
                )?;
                return Ok(
                    serde_json::json!({"version":3,"accepted":true,"request_id":id.to_string(),"job_id":job.id}),
                );
            }
            if request.selection.is_some()
                || request.audio_selection.is_some()
                || request.container.is_some()
                || request.audio_only.is_some()
            {
                bail!("invalid_message");
            }
            if request.version == 2 {
                let status = match request.kind.as_str() {
                    "prepare" => store.prepare_capture(
                        &id.to_string(),
                        request.url.as_deref().context("invalid_message")?,
                        &safe_name(request.filename.as_deref()),
                        &directory,
                        request.etag.as_deref().context("invalid_message")?,
                        request
                            .total
                            .as_deref()
                            .context("invalid_message")?
                            .parse()
                            .context("invalid_message")?,
                    )?,
                    "status" => store.capture_status(&id.to_string())?,
                    "commit" => {
                        if request.browser_cancelled != Some(true) {
                            bail!("cancellation_not_confirmed");
                        }
                        store.commit_capture(&id.to_string())?
                    }
                    "abort" => store.abort_capture(&id.to_string())?,
                    _ => unreachable!(),
                };
                let activation = if request.kind == "commit" {
                    // Ownership is already committed. Launch failures are reported
                    // without releasing the job or resurrecting Chrome's transfer.
                    std::env::current_exe().ok().and_then(|executable| {
                        idmxl_engine::application::activate_capture(
                            &store,
                            &id.to_string(),
                            &executable,
                        )
                        .ok()
                    })
                } else {
                    None
                };
                return Ok(
                    serde_json::json!({"version":2,"accepted":true,"request_id":id.to_string(),"job_id":status.job_id,"phase":status.phase,"expires":status.expires,"activation":activation}),
                );
            }
            if request.etag.is_some()
                || request.total.is_some()
                || request.browser_cancelled.is_some()
            {
                bail!("invalid_message");
            }
            let name = format!("{}-{}", id, safe_name(request.filename.as_deref()));
            let job = store.add_handoff(
                AddJob {
                    url: request.url.context("invalid_message")?,
                    destination: directory.join(name),
                    replayable: false,
                    expected_sha256: None,
                },
                &id.to_string(),
            )?;
            // The transaction is committed before this acknowledgement. Nothing cancels browser downloads.
            Ok(
                serde_json::json!({"version":1,"accepted":true,"request_id":id.to_string(),"job_id":job.id}),
            )
        })();
        let reply = match result {
            Ok(value) => value,
            Err(error) => {
                let code = error.to_string();
                let known = [
                    "invalid_message",
                    "unsupported_message",
                    "invalid_request_id",
                    "handoff_id_conflict",
                    "invalid_url",
                    "unsupported_url_scheme",
                    "credentials_in_url_unsupported",
                    "capture_ineligible",
                    "capture_identity_mismatch",
                    "handoff_not_found",
                    "handoff_not_ready",
                    "handoff_aborted",
                    "handoff_already_committed",
                    "cancellation_not_confirmed",
                    "private_network_blocked",
                ];
                serde_json::json!({"version":1,"accepted":false,"error":if known.contains(&code.as_str()){code.as_str()}else{"handoff_rejected"}})
            }
        };
        let mut encoded = serde_json::to_vec(&reply)?;
        if encoded.len() > MAX_FRAME {
            encoded = serde_json::to_vec(
                &serde_json::json!({"accepted":false,"error":"response_too_large"}),
            )?;
        }
        output.write_all(&(encoded.len() as u32).to_ne_bytes())?;
        output.write_all(&encoded)?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_is_bounded_and_rejects_truncation() {
        assert!(read_frame(&mut &[][..]).unwrap().is_none());
        assert!(read_frame(&mut &[1, 0][..]).is_err());
        assert!(read_frame(&mut &((MAX_FRAME + 1) as u32).to_ne_bytes()[..]).is_err());
        assert!(read_frame(&mut &[3, 0, 0, 0, b'{'][..]).is_err());
        assert_eq!(
            safe_name(Some("../../evil:stream.exe")),
            "_.._evil_stream.exe"
        );
    }
}
