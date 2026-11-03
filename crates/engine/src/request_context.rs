//! A captured request's credentials are protected at rest and never sent to another origin.
use crate::{Store, platform};
use anyhow::{Context, Result, bail};
use curl::easy::{Easy2, Handler, List};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestContext {
    pub origin: String,
    pub headers: Vec<(String, String)>,
    pub content_type: Option<String>,
    #[serde(default)]
    pub expected_etag: Option<String>,
}

impl RequestContext {
    pub fn validate(&self, source: &str) -> Result<()> {
        let url = platform::checked_url(source)?;
        if self.origin != url.origin().ascii_serialization()
            || self.headers.len() > 12
            || serde_json::to_vec(self)?.len() > 64 * 1024
            || self
                .content_type
                .as_ref()
                .is_some_and(|v| v.len() > 256 || v.contains(['\r', '\n']))
            || self.expected_etag.as_ref().is_some_and(|v| {
                v.len() > 1024
                    || v.len() < 2
                    || !v.starts_with('"')
                    || !v.ends_with('"')
                    || v[1..v.len() - 1].contains(['"', '\r', '\n'])
            })
        {
            bail!("invalid_request_context");
        }
        for (name, value) in &self.headers {
            if ![
                "cookie",
                "authorization",
                "referer",
                "origin",
                "user-agent",
                "accept",
                "accept-language",
            ]
            .contains(&name.to_ascii_lowercase().as_str())
                || value.len() > 32768
                || value.contains(['\r', '\n', '\0'])
            {
                bail!("invalid_request_context");
            }
        }
        Ok(())
    }

    pub fn append(&self, url: &str, headers: &mut List) -> Result<()> {
        if platform::checked_url(url)?.origin().ascii_serialization() == self.origin {
            for (name, value) in &self.headers {
                headers.append(&format!("{name}: {value}"))?;
            }
        }
        Ok(())
    }

    pub fn apply<H: Handler>(&self, url: &str, easy: &mut Easy2<H>) -> Result<()> {
        let mut headers = List::new();
        headers.append("Accept-Encoding: identity")?;
        self.append(url, &mut headers)?;
        easy.http_headers(headers)?;
        Ok(())
    }
}

impl Store {
    pub fn capture_context(&self, id: &str) -> Result<Option<RequestContext>> {
        let data: Option<Vec<u8>> = self
            .db
            .query_row(
                "SELECT context FROM request_context WHERE job_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        data.map(|v| Ok(serde_json::from_slice(&platform::unprotect(&v)?)?))
            .transpose()
    }

    pub fn save_capture_context(&self, id: &str, context: &RequestContext) -> Result<()> {
        context.validate(&self.get(id)?.url)?;
        self.db.execute("INSERT INTO request_context VALUES(?1,?2) ON CONFLICT(job_id) DO UPDATE SET context=excluded.context", params![id, platform::protect(&serde_json::to_vec(context)?)?])?;
        Ok(())
    }

    pub fn prepare_native_capture(
        &self,
        request_id: &str,
        url: &str,
        destination: &std::path::Path,
        total: Option<u64>,
        context: &RequestContext,
    ) -> Result<crate::capture::CaptureStatus> {
        uuid::Uuid::parse_str(request_id).context("invalid_request_id")?;
        context.validate(url)?;
        if let Ok(status) = self.capture_status(request_id) {
            if self.get(&status.job_id)?.url != url {
                bail!("handoff_id_conflict");
            }
            if status.phase == "aborted" {
                bail!("handoff_aborted");
            }
            if status.phase != "expired" {
                return Ok(status);
            }
        }
        let probe = crate::transport::probe_context(
            url,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            true,
            false,
            Some(context),
        )?;
        if total.is_some_and(|n| probe.total.is_some_and(|actual| n != actual)) {
            bail!("capture_identity_mismatch");
        }
        if context
            .expected_etag
            .as_ref()
            .is_some_and(|expected| probe.etag.as_ref() != Some(expected))
        {
            bail!("capture_identity_mismatch");
        }
        let expires = crate::store::now() + 600_000;
        let job = self.insert(
            crate::AddJob {
                url: url.into(),
                destination: destination.into(),
                replayable: true,
                expected_sha256: None,
            },
            Some(request_id),
            false,
            "Default",
            Some((
                probe.etag.as_deref().unwrap_or(""),
                probe.total.unwrap_or(0),
                expires,
            )),
            None,
        )?;
        self.save_capture_context(&job.id, context)?;
        self.db.execute(
            "UPDATE capture_handoffs SET expires=?2 WHERE request_id=?1 AND phase='ready'",
            params![request_id, expires],
        )?;
        self.capture_status(request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_seven_history_is_preserved_with_a_readable_pre_upgrade_backup() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let job = store
            .add(crate::AddJob {
                url: "https://example.org/file".into(),
                destination: root.path().join("file.bin"),
                replayable: true,
                expected_sha256: None,
            })
            .unwrap();
        store.request(&job.id, "paused").unwrap();
        store
            .db
            .execute_batch("DROP TABLE request_context; PRAGMA user_version=7;")
            .unwrap();
        drop(store);
        let upgraded = Store::open(root.path()).unwrap();
        assert_eq!(upgraded.view(&job.id).unwrap().state, "paused");
        let backup = std::fs::read_dir(root.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("jobs-v7-")
            })
            .unwrap();
        let old = rusqlite::Connection::open(backup).unwrap();
        assert_eq!(
            old.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            7
        );
        assert_eq!(
            old.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            old.query_row("SELECT id FROM jobs", [], |r| r.get::<_, String>(0))
                .unwrap(),
            job.id
        );
    }
    #[test]
    fn rejects_origin_confusion_and_header_injection() {
        let mut c = RequestContext {
            origin: "https://example.org".into(),
            headers: vec![("cookie".into(), "token=abc".into())],
            content_type: None,
            expected_etag: None,
        };
        assert!(c.validate("https://example.org/file").is_ok());
        assert!(c.validate("https://other.org/file").is_err());
        c.headers[0].1.push_str("\r\nX-Evil: yes");
        assert!(c.validate("https://example.org/file").is_err());
        c.headers = vec![("host".into(), "other.org".into())];
        assert!(c.validate("https://example.org/file").is_err());
    }

    #[test]
    fn headers_never_follow_a_cross_origin_redirect() {
        let c = RequestContext {
            origin: "https://example.org".into(),
            headers: vec![("cookie".into(), "session=secret".into())],
            content_type: None,
            expected_etag: None,
        };
        let mut headers = List::new();
        c.append("https://other.org/file", &mut headers).unwrap();
        assert_eq!(headers.iter().count(), 0);
        c.append("https://example.org/file", &mut headers).unwrap();
        assert_eq!(headers.iter().next().unwrap(), b"cookie: session=secret");
    }

    #[test]
    fn sequential_cookie_dependent_transfer_uses_protected_context_and_verifies_bytes() {
        use sha2::{Digest, Sha256};
        use std::io::{Read, Write};
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let data = vec![77u8; 65536];
        let expected = format!("{:x}", Sha256::digest(&data));
        let server = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while header.len() < 65536 && !header.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    header.push(byte[0]);
                }
                let header = String::from_utf8_lossy(&header).to_ascii_lowercase();
                if !header.contains("cookie: session=secret-context-value") {
                    let _ = stream.write_all(
                        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    continue;
                }
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    data.len()
                );
                let _ = stream.write_all(&data);
            }
        });
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(&root.path().join("state")).unwrap();
        let source = format!("http://{address}/file");
        let job = store
            .add(crate::AddJob {
                url: source.clone(),
                destination: root.path().join("file.bin"),
                replayable: true,
                expected_sha256: Some(expected.clone()),
            })
            .unwrap();
        let context = RequestContext {
            origin: format!("http://{address}"),
            headers: vec![("cookie".into(), "session=secret-context-value".into())],
            content_type: Some("application/octet-stream".into()),
            expected_etag: None,
        };
        store.save_capture_context(&job.id, &context).unwrap();
        let engine = crate::Engine::new(store);
        let result = engine.run(&job.id, Default::default());
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        let completed = result.unwrap();
        assert_eq!(completed.state, "completed");
        assert_eq!(completed.sha256, Some(expected));
        #[cfg(windows)]
        {
            let data: Vec<u8> = engine
                .store
                .db
                .query_row(
                    "SELECT context FROM request_context WHERE job_id=?1",
                    [&job.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                !data
                    .windows(b"secret-context-value".len())
                    .any(|w| w == b"secret-context-value")
            );
        }
    }
}
