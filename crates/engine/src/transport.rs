use std::{
    collections::HashMap,
    fs::File,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, bail};
use curl::easy::{Easy2, Handler, List, WriteError};
use sha2::{Digest, Sha256};

use crate::{model::Extent, platform};

pub(crate) const RECOVERY_BLOCK: u64 = 1024 * 1024;
const WRITE_BATCH: usize = 128 * 1024;

#[derive(Debug, Default, Clone)]
pub struct Headers {
    pub status: u32,
    pub protocol: String,
    pub fields: HashMap<String, String>,
}

impl Headers {
    pub fn length(&self) -> Option<u64> {
        self.fields.get("content-length")?.parse().ok()
    }
    pub fn etag(&self) -> Option<String> {
        let tag = self.fields.get("etag")?;
        (tag.starts_with('"')
            && tag.ends_with('"')
            && tag.len() >= 2
            && !tag[1..tag.len() - 1].contains('"'))
        .then(|| tag.clone())
    }
    pub fn identity_encoding(&self) -> bool {
        self.fields
            .get("content-encoding")
            .is_none_or(|s| s.eq_ignore_ascii_case("identity"))
    }
}

#[derive(Debug, Clone)]
pub enum Mode {
    Probe,
    Sequential,
    Range {
        offset: u64,
        length: u64,
        total: u64,
        etag: String,
    },
}

pub struct Receiver {
    pub mode: Mode,
    pub headers: Headers,
    pub bytes: u64,
    pub error: Option<&'static str>,
    pub hash: Sha256,
    pub(crate) completed: Vec<Extent>,
    pub(crate) block_bytes: u64,
    pub(crate) instrument: bool,
    pub(crate) write_ms: f64,
    pub(crate) hash_ms: f64,
    buffer: Vec<u8>,
    file: Option<Arc<File>>,
    stop: Arc<AtomicBool>,
    ready: bool,
    header_bytes: usize,
}

impl Receiver {
    pub fn new(mode: Mode, file: Option<Arc<File>>, stop: Arc<AtomicBool>) -> Self {
        Self {
            mode,
            headers: Headers::default(),
            bytes: 0,
            error: None,
            hash: Sha256::new(),
            completed: Vec::new(),
            block_bytes: 0,
            instrument: false,
            write_ms: 0.,
            hash_ms: 0.,
            buffer: Vec::new(),
            file,
            stop,
            ready: false,
            header_bytes: 0,
        }
    }
    pub fn digest(&self) -> String {
        format!("{:x}", self.hash.clone().finalize())
    }

    fn write_buffer(&mut self, offset: u64) -> bool {
        if let Some(file) = &self.file {
            let start = self.instrument.then(std::time::Instant::now);
            let result = platform::write_at(file, &self.buffer, offset);
            if let Some(start) = start {
                self.write_ms += crate::performance::milliseconds(start);
            }
            if result.is_err() {
                self.error = Some("storage_write_failed");
                return false;
            }
            crate::fault::point("after_body_write");
        }
        self.buffer.clear();
        true
    }
    fn validate(&self) -> Result<(), &'static str> {
        let status = self.headers.status;
        if (300..400).contains(&status) {
            return Ok(());
        }
        if status == 412 {
            return Err("resource_changed");
        }
        if matches!(status, 429 | 503) {
            return Err("server_throttled");
        }
        if !(200..300).contains(&status) {
            return Err("http_request_failed");
        }
        if !self.headers.identity_encoding() {
            return Err("encoded_response_unsupported");
        }
        match &self.mode {
            Mode::Probe => {
                if status == 206 {
                    let range = self
                        .headers
                        .fields
                        .get("content-range")
                        .and_then(|s| parse_range(s))
                        .ok_or("invalid_content_range")?;
                    if range.0 != 0 || range.1 != 0 || range.2 == 0 {
                        return Err("invalid_content_range");
                    }
                } else if status != 200 {
                    return Err("unexpected_response");
                }
            }
            Mode::Sequential => {
                if status != 200 {
                    return Err("unexpected_partial_response");
                }
            }
            Mode::Range {
                offset,
                length,
                total,
                etag,
            } => {
                if status != 206 {
                    return Err("range_ignored");
                }
                let expected_end = offset
                    .checked_add(*length)
                    .and_then(|n| n.checked_sub(1))
                    .ok_or("invalid_content_range")?;
                if self
                    .headers
                    .fields
                    .get("content-range")
                    .and_then(|s| parse_range(s))
                    != Some((*offset, expected_end, *total))
                {
                    return Err("invalid_content_range");
                }
                if self.headers.etag().as_ref() != Some(etag) {
                    return Err("resource_changed");
                }
                if self.headers.length().is_some_and(|n| n != *length) {
                    return Err("invalid_content_length");
                }
            }
        }
        Ok(())
    }
}

impl Handler for Receiver {
    fn header(&mut self, line: &[u8]) -> bool {
        self.header_bytes += line.len();
        if self.header_bytes > 65536 {
            self.error = Some("headers_too_large");
            return false;
        }
        let Ok(value) = std::str::from_utf8(line) else {
            self.error = Some("invalid_header");
            return false;
        };
        let value = value.trim_end_matches(['\r', '\n']);
        if value.starts_with("HTTP/") {
            self.headers = Headers::default();
            self.headers.protocol = match value.split_whitespace().next().unwrap_or_default() {
                "HTTP/1.0" => "HTTP/1.0",
                "HTTP/1.1" => "HTTP/1.1",
                "HTTP/2" => "HTTP/2",
                "HTTP/3" => "HTTP/3",
                _ => "unknown",
            }
            .to_owned();
            self.ready = false;
            self.headers.status = value
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        } else if value.is_empty() {
            if self.headers.status < 200 {
                return true;
            }
            if let Err(error) = self.validate() {
                self.error = Some(error);
                return false;
            }
            self.ready = true;
        } else if let Some((name, value)) = value.split_once(':') {
            let name = name.to_ascii_lowercase();
            let value = value.trim().to_owned();
            if matches!(
                name.as_str(),
                "content-length" | "content-range" | "etag" | "content-encoding"
            ) && self.headers.fields.contains_key(&name)
            {
                self.error = Some("duplicate_identity_header");
                return false;
            }
            if name == "content-length" && value.parse::<u64>().is_err() {
                self.error = Some("invalid_content_length");
                return false;
            }
            self.headers.fields.insert(name, value);
        }
        true
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        if self.stop.load(Ordering::Relaxed) {
            self.error = Some("transfer_interrupted");
            return Ok(0);
        }
        if !self.ready {
            self.error = Some("unvalidated_body");
            return Ok(0);
        }
        if (300..400).contains(&self.headers.status) {
            // Redirect bodies are unnecessary. The caller validates Location explicitly.
            return Ok(0);
        }
        let (offset, limit) = match self.mode {
            Mode::Probe => {
                if self.headers.status == 200 {
                    return Ok(0);
                }
                (0, Some(1))
            }
            Mode::Sequential => (0, self.headers.length()),
            Mode::Range { offset, length, .. } => (offset, Some(length)),
        };
        let Some(end) = self.bytes.checked_add(data.len() as u64) else {
            self.error = Some("body_too_large");
            return Ok(0);
        };
        if limit.is_some_and(|n| end > n) {
            self.error = Some("body_exceeds_lease");
            return Ok(0);
        }
        if let Mode::Range { offset, length, .. } = self.mode {
            let accepted_before = self.bytes;
            let blocks_before = self.block_bytes;
            let completed_before = self.completed.len();
            let mut remaining = data;
            while !remaining.is_empty() {
                let count = remaining
                    .len()
                    .min(WRITE_BATCH - self.buffer.len())
                    .min((RECOVERY_BLOCK - self.bytes % RECOVERY_BLOCK) as usize);
                self.buffer.extend_from_slice(&remaining[..count]);
                let start = self.instrument.then(std::time::Instant::now);
                self.hash.update(&remaining[..count]);
                if let Some(start) = start {
                    self.hash_ms += crate::performance::milliseconds(start);
                }
                self.bytes += count as u64;
                remaining = &remaining[count..];
                let boundary = self.bytes.is_multiple_of(RECOVERY_BLOCK) || self.bytes == length;
                if (self.buffer.len() == WRITE_BATCH || boundary)
                    && !self.write_buffer(offset + self.bytes - self.buffer.len() as u64)
                {
                    self.bytes = accepted_before;
                    self.block_bytes = blocks_before;
                    self.completed.truncate(completed_before);
                    return Ok(0);
                }
                if boundary {
                    let size = if self.bytes.is_multiple_of(RECOVERY_BLOCK) {
                        RECOVERY_BLOCK
                    } else {
                        self.bytes % RECOVERY_BLOCK
                    };
                    self.completed.push(Extent {
                        offset: offset + self.bytes - size,
                        length: size,
                        digest: format!("{:x}", self.hash.finalize_reset()),
                    });
                    self.block_bytes += size;
                }
            }
            return Ok(data.len());
        }
        if let Some(file) = &self.file {
            let start = self.instrument.then(std::time::Instant::now);
            if platform::write_at(file, data, offset + self.bytes).is_err() {
                self.error = Some("storage_write_failed");
                return Ok(0);
            }
            if let Some(start) = start {
                self.write_ms += crate::performance::milliseconds(start);
            }
            crate::fault::point("after_body_write");
        }
        let start = self.instrument.then(std::time::Instant::now);
        self.hash.update(data);
        if let Some(start) = start {
            self.hash_ms += crate::performance::milliseconds(start);
        }
        self.bytes = end;
        Ok(data.len())
    }

    fn progress(
        &mut self,
        _total: f64,
        _received: f64,
        _upload_total: f64,
        _uploaded: f64,
    ) -> bool {
        if self.stop.load(Ordering::Relaxed) {
            self.error = Some("transfer_interrupted");
            false
        } else {
            true
        }
    }
}

pub fn handle(
    url: &str,
    mode: Mode,
    file: Option<Arc<File>>,
    stop: Arc<AtomicBool>,
    speed: u64,
    public_only: bool,
) -> Result<Easy2<Receiver>> {
    platform::checked_url(url)?;
    let mut headers = List::new();
    headers.append("Accept-Encoding: identity")?;
    if let Mode::Range { etag, .. } = &mode {
        headers.append(&format!("If-Match: {etag}"))?;
    }
    let range = match &mode {
        Mode::Probe => Some("0-0".to_owned()),
        Mode::Range { offset, length, .. } => Some(format!("{offset}-{}", offset + length - 1)),
        Mode::Sequential => None,
    };
    let mut easy = Easy2::new(Receiver::new(mode, file, stop));
    easy.url(url)?;
    if public_only {
        crate::network::pin_public(&mut easy, url)?;
    }
    easy.follow_location(false)?;
    easy.proxy("")?;
    easy.useragent("IDMXL/0.1.0")?;
    easy.http_headers(headers)?;
    easy.http_content_decoding(false)?;
    easy.progress(true)?;
    easy.connect_timeout(Duration::from_secs(15))?;
    easy.low_speed_limit(1)?;
    easy.low_speed_time(Duration::from_secs(30))?;
    easy.ssl_verify_peer(true)?;
    easy.ssl_verify_host(true)?;
    #[cfg(feature = "fault-injection")]
    if let Some(ca) = std::env::var_os("IDMXL_TEST_CA") {
        easy.cainfo(std::path::PathBuf::from(ca))?;
    }
    if let Some(range) = range {
        easy.range(&range)?;
    }
    if speed > 0 {
        easy.max_recv_speed(speed)?;
    }
    Ok(easy)
}

#[derive(Debug)]
pub struct Probe {
    pub resolved: String,
    pub total: Option<u64>,
    pub etag: Option<String>,
    pub ranged: bool,
}

#[derive(Debug)]
pub struct Throttled {
    pub url: String,
    pub delay: Option<Duration>,
}
impl std::fmt::Display for Throttled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "server_throttled")
    }
}
impl std::error::Error for Throttled {}

pub fn probe(url: &str, stop: Arc<AtomicBool>, public_only: bool) -> Result<Probe> {
    probe_inner(url, stop, public_only, true)
}
pub fn probe_exact(url: &str, stop: Arc<AtomicBool>, public_only: bool) -> Result<Probe> {
    probe_inner(url, stop, public_only, false)
}
fn probe_inner(
    url: &str,
    stop: Arc<AtomicBool>,
    public_only: bool,
    redirects: bool,
) -> Result<Probe> {
    let mut resolved = url.to_owned();
    for _ in 0..=5 {
        let mut easy = handle(&resolved, Mode::Probe, None, stop.clone(), 0, public_only)?;
        easy.timeout(Duration::from_secs(20))?;
        let result = easy.perform();
        let receiver = easy.get_ref();
        if stop.load(Ordering::Relaxed) {
            bail!("transfer_interrupted");
        }
        if let Some(error) = receiver.error {
            if error == "server_throttled" {
                return Err(anyhow::Error::new(Throttled {
                    url: resolved,
                    delay: receiver
                        .headers
                        .fields
                        .get("retry-after")
                        .and_then(|n| crate::engine::server_delay(n)),
                }));
            }
            bail!("{error}");
        }
        let status = receiver.headers.status;
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            if !redirects {
                bail!("capture_ineligible");
            }
            let location = receiver
                .headers
                .fields
                .get("location")
                .ok_or_else(|| anyhow::anyhow!("redirect_without_location"))?;
            let next = platform::checked_url(&resolved)?
                .join(location)
                .map_err(|_| anyhow::anyhow!("invalid_redirect"))?;
            platform::checked_url(next.as_str())?;
            if platform::checked_url(&resolved)?.scheme() == "https" && next.scheme() != "https" {
                bail!("insecure_redirect");
            }
            resolved = next.to_string();
            continue;
        }
        if status == 200 {
            // The deliberate early body abort bounds a server that ignored the range probe.
            if receiver
                .headers
                .length()
                .is_some_and(|n| n > i64::MAX as u64)
            {
                bail!("file_too_large");
            }
            return Ok(Probe {
                resolved,
                total: receiver.headers.length(),
                etag: None,
                ranged: false,
            });
        }
        result.map_err(|error| anyhow::anyhow!(transport_error(&error)))?;
        if status != 206 || receiver.bytes != 1 {
            bail!("invalid_probe_response");
        }
        let total = receiver
            .headers
            .fields
            .get("content-range")
            .and_then(|s| parse_range(s))
            .map(|r| r.2);
        if total.is_some_and(|n| n > i64::MAX as u64) {
            bail!("file_too_large");
        }
        let etag = receiver.headers.etag();
        return Ok(Probe {
            resolved,
            total,
            ranged: etag.is_some(),
            etag,
        });
    }
    bail!("too_many_redirects")
}

pub fn transport_error(error: &curl::Error) -> &'static str {
    if matches!(error.code(), 35 | 51 | 58 | 60 | 77) {
        "tls_verification_failed"
    } else {
        "transfer_transport_failed"
    }
}

pub fn parse_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (extent, total) = value.split_once('/')?;
    let (start, end) = extent.split_once('-')?;
    let (start, end, total) = (
        start.parse::<u64>().ok()?,
        end.parse::<u64>().ok()?,
        total.parse::<u64>().ok()?,
    );
    (start <= end && end < total).then_some((start, end, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buffered_range_storage_failure_never_emits_a_recovery_extent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("readonly.bin");
        std::fs::write(&path, b"original").unwrap();
        let mut receiver = Receiver::new(
            Mode::Range {
                offset: 0,
                length: 3,
                total: 3,
                etag: "\"a\"".into(),
            },
            Some(Arc::new(File::open(&path).unwrap())),
            Arc::new(AtomicBool::new(false)),
        );
        for line in [
            b"HTTP/1.1 206 Partial Content\r\n".as_slice(),
            b"Content-Range: bytes 0-2/3\r\n",
            b"Content-Length: 3\r\n",
            b"ETag: \"a\"\r\n",
            b"\r\n",
        ] {
            assert!(receiver.header(line));
        }
        assert_eq!(receiver.write(b"new").unwrap(), 0);
        assert_eq!(receiver.bytes, 0);
        assert_eq!(receiver.block_bytes, 0);
        assert!(receiver.completed.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }
    #[test]
    fn failed_storage_write_does_not_claim_received_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("readonly.bin");
        std::fs::write(&path, b"original").unwrap();
        let file = Arc::new(File::open(&path).unwrap());
        let mut receiver = Receiver::new(
            Mode::Sequential,
            Some(file),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(receiver.header(b"HTTP/1.1 200 OK\r\n"));
        assert!(receiver.header(b"Content-Length: 3\r\n"));
        assert!(receiver.header(b"\r\n"));
        assert_eq!(receiver.write(b"new").unwrap(), 0);
        assert_eq!(receiver.bytes, 0);
        assert_eq!(receiver.error, Some("storage_write_failed"));
        assert_eq!(std::fs::read(path).unwrap(), b"original");
    }
    #[test]
    fn range_bounds_are_checked() {
        assert_eq!(parse_range("bytes 10-19/20"), Some((10, 19, 20)));
        for value in [
            "bytes 20-19/30",
            "bytes 0-20/20",
            "bytes */20",
            "bytes 0-0/*",
            "bytes 0-18446744073709551616/20",
        ] {
            assert!(parse_range(value).is_none());
        }
    }
    #[test]
    fn rejects_changed_identity_before_accepting_body() {
        let mut receiver = Receiver::new(
            Mode::Range {
                offset: 0,
                length: 2,
                total: 2,
                etag: "\"a\"".into(),
            },
            None,
            Arc::new(AtomicBool::new(false)),
        );
        for line in [
            b"HTTP/1.1 206 Partial Content\r\n".as_slice(),
            b"Content-Range: bytes 0-1/2\r\n",
            b"ETag: \"b\"\r\n",
        ] {
            assert!(receiver.header(line));
        }
        assert!(!receiver.header(b"\r\n"));
        assert_eq!(receiver.error, Some("resource_changed"));
        assert_eq!(receiver.bytes, 0);
    }
    #[test]
    fn ignored_range_cannot_be_written_at_an_offset() {
        let mut receiver = Receiver::new(
            Mode::Range {
                offset: 100,
                length: 10,
                total: 200,
                etag: "\"a\"".into(),
            },
            None,
            Arc::new(AtomicBool::new(false)),
        );
        assert!(receiver.header(b"HTTP/1.1 200 OK\r\n"));
        assert!(!receiver.header(b"\r\n"));
        assert_eq!(receiver.error, Some("range_ignored"));
    }
}
