//! Bounded metadata fetches. Bulk media bytes always use the journalled engine.
use crate::platform;
use anyhow::{Result, bail};
use curl::easy::{Easy2, Handler, List, WriteError};
use std::time::Duration;
struct Body {
    bytes: Vec<u8>,
    headers: crate::transport::Headers,
    limit: usize,
    header_bytes: usize,
    failed: bool,
}
impl Handler for Body {
    fn header(&mut self, line: &[u8]) -> bool {
        self.header_bytes += line.len();
        if self.header_bytes > 65536 {
            return false;
        }
        let Ok(line) = std::str::from_utf8(line) else {
            return false;
        };
        let line = line.trim_end_matches(['\r', '\n']);
        if line.starts_with("HTTP/") {
            self.headers = Default::default();
            self.headers.status = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        } else if let Some((key, value)) = line.split_once(':') {
            self.headers
                .fields
                .insert(key.to_ascii_lowercase(), value.trim().into());
        }
        true
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, WriteError> {
        if self.headers.status != 200 {
            return Ok(0);
        }
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            self.failed = true;
            return Ok(0);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
}
pub fn fetch(url: &str, limit: usize, public_only: bool) -> Result<(String, Vec<u8>)> {
    let mut current = platform::checked_url(url)?;
    for _ in 0..=5 {
        let mut easy = Easy2::new(Body {
            bytes: Vec::new(),
            headers: Default::default(),
            limit,
            header_bytes: 0,
            failed: false,
        });
        easy.url(current.as_str())?;
        easy.proxy("")?;
        easy.follow_location(false)?;
        easy.connect_timeout(Duration::from_secs(10))?;
        easy.timeout(Duration::from_secs(20))?;
        easy.ssl_verify_peer(true)?;
        easy.ssl_verify_host(true)?;
        easy.http_content_decoding(false)?;
        let mut headers = List::new();
        headers.append("Accept-Encoding: identity")?;
        easy.http_headers(headers)?;
        if public_only {
            crate::network::pin_public(&mut easy, current.as_str())?;
        }
        #[cfg(feature = "fault-injection")]
        if let Some(ca) = std::env::var_os("IDMXL_TEST_CA") {
            easy.cainfo(std::path::PathBuf::from(ca))?;
        }
        let result = easy.perform();
        let body = easy.get_ref();
        if body.failed {
            bail!("metadata_too_large");
        }
        if matches!(body.headers.status, 301 | 302 | 303 | 307 | 308) {
            let next = current.join(
                body.headers
                    .fields
                    .get("location")
                    .ok_or_else(|| anyhow::anyhow!("invalid_redirect"))?,
            )?;
            platform::checked_url(next.as_str())?;
            if current.scheme() == "https" && next.scheme() != "https" {
                bail!("insecure_redirect");
            }
            current = next;
            continue;
        }
        result.map_err(|_| anyhow::anyhow!("metadata_request_failed"))?;
        if body.headers.status != 200 || !body.headers.identity_encoding() {
            bail!("metadata_request_failed");
        }
        return Ok((current.to_string(), body.bytes.clone()));
    }
    bail!("too_many_redirects")
}
