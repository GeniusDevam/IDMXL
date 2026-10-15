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

use crate::platform;

#[derive(Debug, Default, Clone)]
pub struct Headers {
    pub status: u32,
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
            file,
            stop,
            ready: false,
            header_bytes: 0,
        }
    }
    pub fn digest(&self) -> String {
        format!("{:x}", self.hash.clone().finalize())
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
        if let Some(file) = &self.file
            && platform::write_at(file, data, offset + self.bytes).is_err()
        {
            self.error = Some("storage_write_failed");
            return Ok(0);
        }
        self.hash.update(data);
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
        !self.stop.load(Ordering::Relaxed)
    }
}

pub fn handle(
    url: &str,
    mode: Mode,
    file: Option<Arc<File>>,
    stop: Arc<AtomicBool>,
    speed: u64,
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

pub fn probe(url: &str, stop: Arc<AtomicBool>) -> Result<Probe> {
    let mut resolved = url.to_owned();
    for _ in 0..=5 {
        let easy = handle(&resolved, Mode::Probe, None, stop.clone(), 0)?;
        let result = easy.perform();
        let receiver = easy.get_ref();
        if stop.load(Ordering::Relaxed) {
            bail!("transfer_interrupted");
        }
        if let Some(error) = receiver.error {
            bail!("{error}");
        }
        let status = receiver.headers.status;
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
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
        result.map_err(|_| anyhow::anyhow!("probe_transport_failed"))?;
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
