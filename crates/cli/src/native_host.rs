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
    url: String,
    filename: Option<String>,
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
            if request.version != 1 || request.kind != "enqueue" {
                bail!("unsupported_message");
            }
            let id = uuid::Uuid::parse_str(&request.request_id).context("invalid_request_id")?;
            let name = format!("{}-{}", id, safe_name(request.filename.as_deref()));
            let job = store.add_handoff(
                AddJob {
                    url: request.url,
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
                ];
                serde_json::json!({"version":1,"accepted":false,"error":if known.contains(&code.as_str()){code.as_str()}else{"handoff_rejected"}})
            }
        };
        let encoded = serde_json::to_vec(&reply)?;
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
