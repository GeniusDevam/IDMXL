use std::{collections::HashSet, path::PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Local, NaiveDateTime, Timelike};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{AddJob, JobView, Store, platform};

/// A recurring admission window in the computer's local civil time.
/// Bit zero is Monday. Equal start/end means the entire selected calendar day.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueWindow {
    pub days: u8,
    pub start_minute: u16,
    pub end_minute: u16,
}

impl QueueWindow {
    pub fn validate(&self) -> Result<()> {
        if self.days == 0 || self.days > 127 || self.start_minute >= 1440 || self.end_minute >= 1440
        {
            bail!("invalid_queue_window");
        }
        Ok(())
    }

    pub fn allows_local(&self, local: NaiveDateTime) -> bool {
        let day = local.weekday().num_days_from_monday();
        let minute = local.hour() * 60 + local.minute();
        let selected = |day: u32| self.days & (1 << day) != 0;
        let start = u32::from(self.start_minute);
        let end = u32::from(self.end_minute);
        if start == end {
            selected(day)
        } else if start < end {
            selected(day) && minute >= start && minute < end
        } else {
            (selected(day) && minute >= start) || (selected((day + 6) % 7) && minute < end)
        }
    }
}

pub(crate) fn checked_queue_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) {
        bail!("invalid_queue_name");
    }
    Ok(name)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderRule {
    pub folder: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderSettings {
    pub directory: PathBuf,
    pub organize: bool,
    pub rules: Vec<FolderRule>,
}

impl FolderSettings {
    fn normalized(&self) -> Result<Self> {
        let directory = self
            .directory
            .canonicalize()
            .context("destination_folder_missing")?;
        if !directory.is_dir() || platform::is_reparse(&directory)? || self.rules.len() > 20 {
            bail!("invalid_folder_settings");
        }
        let mut folders = HashSet::new();
        let mut extensions = HashSet::new();
        let mut rules = Vec::new();
        for rule in &self.rules {
            let folder = rule.folder.trim().to_owned();
            platform::validate_filename(&folder).context("invalid_folder_rule")?;
            if folder.encode_utf16().count() > 80
                || !folders.insert(folder.to_lowercase())
                || rule.extensions.is_empty()
                || rule.extensions.len() > 100
                || folder.contains(['/', '\\'])
            {
                bail!("invalid_folder_rule");
            }
            let mut normalized = Vec::new();
            for extension in &rule.extensions {
                let extension = extension.trim();
                let extension = extension
                    .strip_prefix('.')
                    .unwrap_or(extension)
                    .to_ascii_lowercase();
                if extension.is_empty()
                    || extension.len() > 24
                    || !extension
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'.')
                    || extension.starts_with('.')
                    || extension.ends_with('.')
                    || extension.contains("..")
                    || !extensions.insert(extension.clone())
                {
                    bail!("invalid_folder_rule");
                }
                normalized.push(extension);
            }
            rules.push(FolderRule {
                folder,
                extensions: normalized,
            });
        }
        Ok(Self {
            directory,
            organize: self.organize,
            rules,
        })
    }

    fn folder_for(&self, filename: &str) -> Result<PathBuf> {
        if !self.organize {
            return Ok(self.directory.clone());
        }
        let lower = filename.to_ascii_lowercase();
        let folder = self
            .rules
            .iter()
            .find(|r| {
                r.extensions
                    .iter()
                    .any(|e| lower.ends_with(&format!(".{e}")))
            })
            .map_or("Other", |r| r.folder.as_str());
        let path = self.directory.join(folder);
        match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => bail!("category_folder_unavailable"),
        }
        if platform::is_reparse(&path)? || !path.is_dir() {
            bail!("category_folder_unavailable");
        }
        let canonical = path.canonicalize().context("category_folder_unavailable")?;
        if canonical.parent() != Some(self.directory.as_path()) {
            bail!("category_folder_unavailable");
        }
        Ok(canonical)
    }
}

pub fn default_folder_rules() -> Vec<FolderRule> {
    [
        ("Documents", "pdf,doc,docx,xls,xlsx,ppt,pptx,txt,csv,epub"),
        ("Archives", "zip,7z,rar,gz,bz2,xz,tar"),
        ("Images", "jpg,jpeg,png,gif,webp,avif,svg,tiff,bmp"),
        ("Music", "mp3,m4a,aac,flac,ogg,wav,opus"),
        ("Videos", "mp4,mkv,webm,mov,avi,m4v"),
        ("Programs", "exe,msi,msix,msixbundle,appx,appxbundle"),
    ]
    .into_iter()
    .map(|(folder, extensions)| FolderRule {
        folder: folder.into(),
        extensions: extensions.split(',').map(str::to_owned).collect(),
    })
    .collect()
}

fn filename(url: &url::Url) -> String {
    let encoded = url
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .unwrap_or("");
    let decoded = percent_encoding::percent_decode_str(encoded).decode_utf8_lossy();
    let mut name = String::new();
    for c in decoded.chars() {
        let c = if c.is_control()
            || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        {
            '_'
        } else {
            c
        };
        if name.encode_utf16().count() + c.len_utf16() > 180 {
            break;
        }
        name.push(c);
    }
    let name = name.trim_matches(['.', ' ']);
    if name.is_empty() {
        return "download.bin".into();
    }
    if platform::validate_filename(name).is_err() {
        format!("_{name}")
    } else {
        name.into()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkBatch {
    pub links: Vec<String>,
    pub folders: FolderSettings,
    #[serde(default)]
    pub replayable: bool,
    #[serde(default = "default_queue")]
    pub queue_name: String,
}

fn default_queue() -> String {
    "Default".into()
}

#[derive(Debug, Serialize)]
pub struct LinkResult {
    pub index: usize,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<JobView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Store {
    pub fn queue_window(&self, queue: &str) -> Result<Option<QueueWindow>> {
        let queue = checked_queue_name(queue)?;
        let value: Option<String> = self
            .db
            .query_row(
                "SELECT schedule FROM queue_windows WHERE queue_name=?1",
                [queue],
                |r| r.get(0),
            )
            .optional()?;
        value
            .map(|value| {
                let window: QueueWindow = serde_json::from_str(&value)?;
                window.validate()?;
                Ok(window)
            })
            .transpose()
    }

    pub fn set_queue_window(&self, queue: &str, window: Option<&QueueWindow>) -> Result<()> {
        let queue = checked_queue_name(queue)?;
        if let Some(window) = window {
            window.validate()?;
            self.db.execute("INSERT INTO queue_windows VALUES (?1,?2) ON CONFLICT(queue_name) DO UPDATE SET schedule=excluded.schedule", params![queue, serde_json::to_string(window)?])?;
        } else {
            self.db
                .execute("DELETE FROM queue_windows WHERE queue_name=?1", [queue])?;
        }
        Ok(())
    }

    pub fn queue_open(&self, queue: &str) -> Result<bool> {
        Ok(self
            .queue_window(queue)?
            .is_none_or(|window| window.allows_local(Local::now().naive_local())))
    }

    pub fn folder_settings(&self) -> Result<Option<FolderSettings>> {
        let value: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM settings WHERE key='download_folders'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        value
            .map(|value| Ok(serde_json::from_str(&value)?))
            .transpose()
    }

    pub fn set_folder_settings(&self, settings: &FolderSettings) -> Result<FolderSettings> {
        let settings = settings.normalized()?;
        self.db.execute("INSERT INTO settings VALUES ('download_folders',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&settings)?])?;
        Ok(settings)
    }

    pub fn add_links(&self, batch: LinkBatch) -> Result<Vec<LinkResult>> {
        if batch.links.is_empty()
            || batch.links.len() > 1000
            || batch.links.iter().map(String::len).sum::<usize>() > 1_048_576
        {
            bail!("invalid_batch_size");
        }
        let folders = batch.folders.normalized()?;
        let queue = checked_queue_name(&batch.queue_name)?;
        let mut seen = HashSet::new();
        let mut results = Vec::new();
        for (index, source) in batch.links.into_iter().enumerate() {
            let result = (|| {
                let mut url = platform::checked_url(source.trim())?;
                url.set_fragment(None);
                if !seen.insert(url.to_string()) {
                    return Ok(None);
                }
                let name = filename(&url);
                let directory = folders.folder_for(&name)?;
                self.add_generated(
                    AddJob {
                        url: url.into(),
                        destination: directory.join(name),
                        replayable: batch.replayable,
                        expected_sha256: None,
                    },
                    queue,
                )
                .map(Some)
            })();
            results.push(match result {
                Ok(Some(job)) => LinkResult {
                    index,
                    status: "queued",
                    job: Some(job),
                    error: None,
                },
                Ok(None) => LinkResult {
                    index,
                    status: "duplicate",
                    job: None,
                    error: None,
                },
                Err(error) => {
                    let code = error.to_string();
                    let safe = [
                        "invalid_url",
                        "unsupported_protocol",
                        "url_credentials_unsupported",
                        "url_too_large",
                        "category_folder_unavailable",
                        "no_available_filename",
                    ];
                    LinkResult {
                        index,
                        status: "rejected",
                        job: None,
                        error: Some(if safe.contains(&code.as_str()) {
                            code
                        } else {
                            "download_could_not_be_added".into()
                        }),
                    }
                }
            });
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_include_start_exclude_end_and_cross_week_boundaries() {
        let window = QueueWindow {
            days: 1 << 6,
            start_minute: 22 * 60,
            end_minute: 6 * 60,
        };
        let date = |value| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").unwrap();
        assert!(!window.allows_local(date("2026-10-18 21:59")));
        assert!(window.allows_local(date("2026-10-18 22:00")));
        assert!(window.allows_local(date("2026-10-19 05:59")));
        assert!(!window.allows_local(date("2026-10-19 06:00")));
        assert!(!window.allows_local(date("2026-10-20 02:00")));
        let all_day = QueueWindow {
            days: 1,
            start_minute: 0,
            end_minute: 0,
        };
        assert!(all_day.allows_local(date("2026-10-19 23:59")));
        assert!(!all_day.allows_local(date("2026-10-20 00:00")));
        let daytime = QueueWindow {
            days: 1,
            start_minute: 9 * 60,
            end_minute: 17 * 60,
        };
        assert!(daytime.allows_local(date("2026-10-19 09:00")));
        assert!(!daytime.allows_local(date("2026-10-19 17:00")));
        assert!(
            QueueWindow {
                days: 0,
                ..all_day.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            QueueWindow {
                start_minute: 1440,
                ..all_day
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn filenames_decode_without_using_query_tokens_or_path_separators() {
        for (url, expected) in [
            (
                "https://example.org/report%20one.PDF?token=secret",
                "report one.PDF",
            ),
            ("https://example.org/%2e%2e%5cCON%3afoo.exe", "_CON_foo.exe"),
            ("https://example.org/CON.txt", "_CON.txt"),
            ("https://example.org/", "download.bin"),
        ] {
            let name = filename(&url::Url::parse(url).unwrap());
            assert_eq!(name, expected);
            assert!(platform::validate_filename(&name).is_ok());
            assert!(!name.contains("secret"));
        }
    }
}
