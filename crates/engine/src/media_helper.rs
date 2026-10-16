//! Fixed, hash-pinned, network-free helpers. Job limits are resource containment, not a sandbox.
use crate::platform;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const FFMPEG_HASH: &str = match option_env!("IDMXL_MEDIA_FFMPEG_SHA256") {
    Some(hash) => hash,
    None => "292f03f3d20d990aa6c09c2c420f049a8c2690c7c965eb48f18e6354cd753f77",
};
const FFPROBE_HASH: &str = match option_env!("IDMXL_MEDIA_FFPROBE_SHA256") {
    Some(hash) => hash,
    None => "e1f6ee0a87b3bebfcad3a2c50a861006c8213a71cadc19f4459395fb3374a85f",
};
pub fn paths() -> Result<(PathBuf, PathBuf)> {
    let root = std::env::current_exe()?
        .parent()
        .context("media_helper_missing")?
        .join("media");
    #[cfg(feature = "fault-injection")]
    let root = std::env::var_os("IDMXL_TEST_MEDIA_DIR")
        .map(PathBuf::from)
        .unwrap_or(root);
    let ffmpeg = root.join("ffmpeg.exe");
    let ffprobe = root.join("ffprobe.exe");
    for (path, expected) in [(&ffmpeg, FFMPEG_HASH), (&ffprobe, FFPROBE_HASH)] {
        if !path.is_file() {
            bail!("media_helper_missing");
        }
        if platform::is_reparse(path)? {
            bail!("media_helper_untrusted");
        }
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        if size > 32 * 1024 * 1024 || platform::hash_extent(&file, 0, size)? != expected {
            bail!("media_helper_untrusted");
        }
    }
    Ok((ffmpeg, ffprobe))
}

#[cfg(windows)]
struct ProcessGroup(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl ProcessGroup {
    fn new(child: &std::process::Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            bail!("media_helper_failed");
        }
        let group = Self(handle);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        limits.ProcessMemoryLimit = 512 * 1024 * 1024;
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(handle, child.as_raw_handle()) } == 0
        {
            bail!("media_helper_failed");
        }
        Ok(group)
    }
}
#[cfg(windows)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
fn run(
    program: &Path,
    workspace: &Path,
    args: &[String],
    stop: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    if platform::is_reparse(workspace)? {
        bail!("media_workspace_changed");
    }
    let mut command = Command::new(program);
    command
        .current_dir(workspace)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Retain only Windows loader/runtime variables; helpers never inherit proxy or media options.
    command.env_clear();
    for key in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().context("media_helper_failed")?;
    #[cfg(windows)]
    let _group = match ProcessGroup::new(&child) {
        Ok(group) => group,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let stdout = child.stdout.take().context("media_helper_failed")?;
    let excessive = Arc::new(AtomicBool::new(false));
    let over_limit = excessive.clone();
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let result = stdout
            .take(1024 * 1024 + 1)
            .read_to_end(&mut output)
            .map(|_| output);
        if result.as_ref().is_ok_and(|bytes| bytes.len() > 1024 * 1024) {
            over_limit.store(true, Ordering::Relaxed);
        }
        result
    });
    let started = Instant::now();
    let outcome = loop {
        if excessive.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(anyhow::anyhow!("media_helper_output_too_large"));
        }
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(anyhow::anyhow!("transfer_interrupted"));
        }
        if started.elapsed() > Duration::from_secs(1800) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(anyhow::anyhow!("media_helper_timeout"));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("media_helper_failed"))
                };
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(anyhow::anyhow!("media_helper_failed"));
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let output = reader
        .join()
        .map_err(|_| anyhow::anyhow!("media_helper_failed"))?
        .context("media_helper_failed")?;
    outcome?;
    if output.len() > 1024 * 1024 {
        bail!("media_helper_output_too_large");
    }
    Ok(output)
}
const FORMATS: &str = "mov,matroska,mpegts,aac,mp3,ogg,wav";
pub fn probe(program: &Path, workspace: &Path, name: &str, stop: Arc<AtomicBool>) -> Result<Value> {
    let args=["-v","error","-protocol_whitelist","file","-format_whitelist",FORMATS,"-count_packets","-show_entries","stream=index,codec_type,codec_name,codec_tag_string,nb_read_packets,duration_ts,time_base,width,height,channels,sample_rate,color_space,color_transfer,color_primaries:stream_tags=language:stream_side_data:format=format_name,duration","-of","json",name].map(String::from);
    let output = run(program, workspace, &args, stop)?;
    serde_json::from_slice(&output).context("media_verification_failed")
}
fn stream<'a>(metadata: &'a Value, role: &str) -> Option<&'a Value> {
    metadata
        .get("streams")?
        .as_array()?
        .iter()
        .find(|s| s.get("codec_type").and_then(Value::as_str) == Some(role))
}
fn seconds(metadata: &Value) -> Option<f64> {
    metadata
        .get("format")?
        .get("duration")?
        .as_str()?
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.0)
}
pub fn validate_source(
    metadata: &Value,
    role: &str,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<f64>,
    audio_only: bool,
) -> Result<()> {
    let required = if audio_only || role == "audio" {
        "audio"
    } else {
        "video"
    };
    let selected = stream(metadata, required).context("media_verification_failed")?;
    if matches!(
        selected.get("codec_tag_string").and_then(Value::as_str),
        Some("encv" | "enca")
    ) || serde_json::to_string(metadata)?
        .to_ascii_lowercase()
        .contains("encryption")
    {
        bail!("unsupported_media_protection_or_timeline");
    }
    if selected
        .get("codec_name")
        .and_then(Value::as_str)
        .is_none_or(|s| s == "unknown")
    {
        bail!("media_verification_failed");
    }
    if !audio_only
        && (width.is_some_and(|w| selected.get("width").and_then(Value::as_u64) != Some(w.into()))
            || height
                .is_some_and(|h| selected.get("height").and_then(Value::as_u64) != Some(h.into())))
    {
        bail!("media_verification_failed");
    }
    if let Some(expected) = duration
        && seconds(metadata)
            .is_none_or(|actual| (actual - expected).abs() > 1.0f64.max(expected * 0.01))
    {
        bail!("media_verification_failed");
    }
    Ok(())
}
pub struct RemuxSettings<'a> {
    pub container: &'a str,
    pub audio_only: bool,
    pub roles: &'a [&'a str],
    pub aac_to_asc: bool,
}
pub fn remux(
    program: &Path,
    workspace: &Path,
    inputs: &[String],
    output: &str,
    settings: RemuxSettings<'_>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let RemuxSettings {
        container,
        audio_only,
        roles,
        aac_to_asc,
    } = settings;
    let mut args = ["-nostdin", "-v", "error", "-n"].map(String::from).to_vec();
    for input in inputs {
        args.extend(
            [
                "-protocol_whitelist",
                "file",
                "-format_whitelist",
                FORMATS,
                "-i",
                input,
            ]
            .map(String::from),
        );
    }
    if audio_only {
        let index = roles.iter().position(|r| *r == "audio").unwrap_or(0);
        args.extend(["-map".into(), format!("{index}:a:0")]);
    } else {
        for (index, role) in roles.iter().enumerate() {
            match *role {
                "audio" => args.extend(["-map".into(), format!("{index}:a:0")]),
                "video" => args.extend(["-map".into(), format!("{index}:v:0")]),
                "muxed" => args.extend([
                    "-map".into(),
                    format!("{index}:v:0"),
                    "-map".into(),
                    format!("{index}:a:0?"),
                ]),
                _ => bail!("invalid_media_selection"),
            }
        }
    }
    if aac_to_asc && matches!(container, "mp4" | "mov" | "ipod") {
        args.extend(["-bsf:a", "aac_adtstoasc"].map(String::from));
    }
    args.extend(
        [
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
            "-c",
            "copy",
            "-f",
            container,
            output,
        ]
        .map(String::from),
    );
    run(program, workspace, &args, stop)?;
    Ok(())
}
pub fn uses_aac(inputs: &[Value], roles: &[&str]) -> bool {
    let input = inputs
        .iter()
        .zip(roles)
        .find(|(_, r)| **r == "audio")
        .or_else(|| inputs.iter().zip(roles).find(|(_, r)| **r == "muxed"));
    input.is_some_and(|(metadata, _)| {
        stream(metadata, "audio")
            .and_then(|a| a.get("codec_name"))
            .and_then(Value::as_str)
            == Some("aac")
            && matches!(
                metadata
                    .get("format")
                    .and_then(|f| f.get("format_name"))
                    .and_then(Value::as_str),
                Some("aac" | "mpegts")
            )
    })
}
pub fn validate_output(
    inputs: &[Value],
    output: &Value,
    roles: &[&str],
    audio_only: bool,
) -> Result<()> {
    let video = if audio_only {
        None
    } else {
        inputs
            .iter()
            .zip(roles)
            .filter(|(_, r)| **r != "audio")
            .find_map(|(m, _)| stream(m, "video"))
    };
    let audio = inputs
        .iter()
        .zip(roles)
        .filter(|(_, r)| **r == "audio")
        .find_map(|(m, _)| stream(m, "audio"))
        .or_else(|| {
            inputs
                .iter()
                .zip(roles)
                .filter(|(_, r)| **r == "muxed")
                .find_map(|(m, _)| stream(m, "audio"))
        });
    for (role, expected) in [("video", video), ("audio", audio)] {
        let actual = stream(output, role);
        if expected.is_some() != actual.is_some() {
            bail!("media_verification_failed");
        }
        if let (Some(expected), Some(actual)) = (expected, actual) {
            for field in [
                "codec_name",
                "width",
                "height",
                "channels",
                "sample_rate",
                "color_space",
                "color_transfer",
                "color_primaries",
                "side_data_list",
                "nb_read_packets",
            ] {
                if field == "nb_read_packets"
                    && expected
                        .get("codec_name")
                        .and_then(Value::as_str)
                        .is_some_and(|c| c.starts_with("pcm_"))
                {
                    continue;
                }
                if expected.get(field) != actual.get(field) {
                    bail!("media_verification_failed");
                }
            }
            if expected
                .get("codec_name")
                .and_then(Value::as_str)
                .is_some_and(|c| c.starts_with("pcm_"))
                && (pcm_samples(expected).is_none() || pcm_samples(expected) != pcm_samples(actual))
            {
                bail!("media_verification_failed");
            }
        }
    }
    if output
        .get("streams")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        != usize::from(video.is_some()) + usize::from(audio.is_some())
    {
        bail!("media_verification_failed");
    }
    let longest = inputs.iter().filter_map(seconds).fold(0.0f64, f64::max);
    if longest > 0.0
        && seconds(output).is_none_or(|n| (n - longest).abs() > 1.0f64.max(longest * 0.01))
    {
        bail!("media_verification_failed");
    }
    Ok(())
}
fn pcm_samples(stream: &Value) -> Option<u128> {
    let duration = stream.get("duration_ts")?.as_u64()? as u128;
    let (numerator, denominator) = stream.get("time_base")?.as_str()?.split_once('/')?;
    let numerator = numerator.parse::<u128>().ok()?;
    let denominator = denominator.parse::<u128>().ok()?;
    let rate = stream.get("sample_rate")?.as_str()?.parse::<u128>().ok()?;
    let count = duration.checked_mul(numerator)?.checked_mul(rate)?;
    (denominator > 0 && count % denominator == 0).then(|| count / denominator)
}

pub fn validate_file_kind(path: &Path, flavor: &str) -> Result<()> {
    if flavor == "direct" {
        return Ok(());
    }
    let mut file = File::open(path)?;
    let mut prefix = [0u8; 8];
    file.read_exact(&mut prefix)
        .context("media_verification_failed")?;
    let mp4 = &prefix[4..8] == b"ftyp";
    let ts = prefix[0] == 0x47 && file.metadata()?.len() % 188 == 0;
    let adts = prefix[0] == 0xff && prefix[1] & 0xf6 == 0xf0;
    if !(mp4 || flavor == "hls" && (ts || adts)) {
        bail!("unsupported_media_segment_format");
    }
    Ok(())
}
