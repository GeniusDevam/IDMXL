//! Signed release metadata, a monotonic receipt and paired, non-destructive recovery.
use crate::{AddJob, Engine, RunOptions, Store, platform, small_http};
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseConfig {
    pub endpoint: String,
    pub artifact_origin: String,
    pub public_key: String,
    pub certificate_thumbprint: String,
    pub channel: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub product: String,
    pub channel: String,
    pub target: String,
    pub version: String,
    pub sequence: u64,
    pub issued: i64,
    pub expires: i64,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub notes: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub payload: String,
    pub signature: String,
}
#[derive(Default, Serialize, Deserialize)]
struct Watermark {
    sequence: u64,
    payload_hash: String,
    issued: i64,
}
#[derive(Serialize)]
pub struct UpdateView {
    pub configured: bool,
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}
fn https(value: &str) -> Result<url::Url> {
    let u = platform::checked_url(value)?;
    if u.scheme() != "https" || u.fragment().is_some() {
        bail!("invalid_release_configuration");
    }
    Ok(u)
}
pub fn configuration() -> Result<Option<ReleaseConfig>> {
    let Some(text) = option_env!("IDMXL_RELEASE_CONFIG") else {
        return Ok(None);
    };
    let config: ReleaseConfig =
        serde_json::from_str(text).context("invalid_release_configuration")?;
    validate_config(&config)?;
    Ok(Some(config))
}
fn validate_config(config: &ReleaseConfig) -> Result<()> {
    https(&config.endpoint)?;
    let origin = https(&config.artifact_origin)?;
    if origin.as_str().trim_end_matches('/') != origin.origin().ascii_serialization()
        || config.channel != "stable"
        || config.certificate_thumbprint.len() != 40
        || !config
            .certificate_thumbprint
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || !STANDARD
            .decode(&config.public_key)
            .is_ok_and(|b| b.len() == 32)
    {
        bail!("invalid_release_configuration");
    }
    Ok(())
}
fn verify(
    config: &ReleaseConfig,
    bytes: &[u8],
    current: &str,
    now: i64,
    watermark: &Watermark,
) -> Result<Manifest> {
    validate_config(config)?;
    if bytes.len() > 96 * 1024 {
        bail!("invalid_update_manifest");
    }
    let envelope: Envelope = serde_json::from_slice(bytes).context("invalid_update_manifest")?;
    let payload = STANDARD
        .decode(envelope.payload)
        .context("invalid_update_signature")?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(envelope.signature)
            .context("invalid_update_signature")?,
    )
    .context("invalid_update_signature")?;
    let key: [u8; 32] = STANDARD
        .decode(&config.public_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid_release_configuration"))?;
    VerifyingKey::from_bytes(&key)
        .context("invalid_release_configuration")?
        .verify_strict(&payload, &signature)
        .context("invalid_update_signature")?;
    let release: Manifest = serde_json::from_slice(&payload).context("invalid_update_manifest")?;
    let version = semver::Version::parse(&release.version).context("invalid_update_manifest")?;
    let current = semver::Version::parse(current).context("invalid_update_manifest")?;
    let url = https(&release.url)?;
    if release.sequence == 0
        || release.product != "dev.idmxl.desktop"
        || release.channel != config.channel
        || release.target != "windows-x86_64"
        || !version.pre.is_empty()
        || !version.build.is_empty()
        || url.origin().ascii_serialization() != config.artifact_origin.trim_end_matches('/')
        || url.query().is_some()
        || release.size == 0
        || release.size > 512 * 1024 * 1024
        || release.sha256.len() != 64
        || !release.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || release.notes.len() > 8192
    {
        bail!("invalid_update_manifest");
    }
    if release.issued > now.saturating_add(300_000)
        || release.expires <= now
        || release.expires <= release.issued
        || release.expires.saturating_sub(release.issued) > 14 * 86_400_000
        || now < watermark.issued.saturating_sub(300_000)
    {
        bail!("update_metadata_expired_or_clock_invalid");
    }
    let payload_hash = format!("{:x}", Sha256::digest(&payload));
    if release.sequence < watermark.sequence
        || release.sequence == watermark.sequence
            && watermark.sequence != 0
            && watermark.payload_hash != payload_hash
        || release.issued < watermark.issued
        || version < current
    {
        bail!("update_rollback_rejected");
    }
    Ok(release)
}
fn watermark(store: &Store) -> Result<Watermark> {
    use rusqlite::OptionalExtension;
    let bytes: Option<String> = store
        .db
        .query_row(
            "SELECT value FROM settings WHERE key='update_watermark'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match bytes {
        None => Ok(Watermark::default()),
        Some(s) => serde_json::from_str(&s).context("invalid_update_receipt"),
    }
}
pub fn check(profile: &Path) -> Result<UpdateView> {
    let Some(config) = configuration()? else {
        return Ok(UpdateView {
            configured: false,
            available: false,
            version: None,
            notes: None,
        });
    };
    let (_, bytes) = small_http::fetch(&config.endpoint, 96 * 1024, true)?;
    let store = Store::open(profile)?;
    let tx = store.write_transaction()?;
    let release = verify(
        &config,
        &bytes,
        env!("CARGO_PKG_VERSION"),
        crate::store::now(),
        &watermark(&store)?,
    )?;
    let envelope: Envelope = serde_json::from_slice(&bytes)?;
    let mark = Watermark {
        sequence: release.sequence,
        payload_hash: format!("{:x}", Sha256::digest(STANDARD.decode(envelope.payload)?)),
        issued: release.issued,
    };
    tx.execute("INSERT INTO settings VALUES('update_watermark',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&mark)?])?;
    tx.execute("INSERT INTO settings VALUES('update_candidate',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[String::from_utf8(bytes)?])?;
    tx.commit()?;
    Ok(UpdateView {
        configured: true,
        available: semver::Version::parse(&release.version)?
            > semver::Version::parse(env!("CARGO_PKG_VERSION"))?,
        version: Some(release.version),
        notes: Some(release.notes),
    })
}
pub fn download(profile: &Path, version: &str) -> Result<PathBuf> {
    let config = configuration()?.context("updates_not_configured")?;
    let store = Store::open(profile)?;
    let bytes: String = store
        .db
        .query_row(
            "SELECT value FROM settings WHERE key='update_candidate'",
            [],
            |r| r.get(0),
        )
        .context("check_updates_first")?;
    let release = verify(
        &config,
        bytes.as_bytes(),
        env!("CARGO_PKG_VERSION"),
        crate::store::now(),
        &watermark(&store)?,
    )?;
    if release.version != version
        || semver::Version::parse(&release.version)?
            <= semver::Version::parse(env!("CARGO_PKG_VERSION"))?
    {
        bail!("update_selection_changed");
    }
    let speed = store.global_speed_limit()?;
    drop(store);
    let directory = profile
        .join("updates")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&directory)?;
    let transfer = Store::open(&directory.join("transfer"))?;
    let destination = directory.join("IDMXL-setup.exe");
    let job = transfer.add(AddJob {
        url: release.url,
        destination: destination.clone(),
        replayable: true,
        expected_sha256: Some(release.sha256),
    })?;
    transfer
        .db
        .execute("UPDATE jobs SET public_only=1 WHERE id=?1", [&job.id])?;
    let result = Engine::new(transfer).run(
        &job.id,
        RunOptions {
            speed_limit_bytes: speed,
            ..Default::default()
        },
    )?;
    if result.state != "completed"
        || result.total_bytes.as_deref() != Some(release.size.to_string().as_str())
    {
        bail!("update_size_mismatch");
    }
    verify_authenticode(&destination, &config.certificate_thumbprint)?;
    let latest = Store::open(profile)?;
    verify(
        &config,
        bytes.as_bytes(),
        env!("CARGO_PKG_VERSION"),
        crate::store::now(),
        &watermark(&latest)?,
    )?;
    Ok(destination)
}
pub fn verify_authenticode(path: &Path, thumbprint: &str) -> Result<()> {
    let directory = path.parent().context("invalid_update_artifact")?;
    let script = directory.join(format!("verify-{}.ps1", uuid::Uuid::new_v4()));
    use std::io::Write;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&script)?;
    file.write_all(include_bytes!("../../../scripts/verify-authenticode.ps1"))?;
    file.sync_all()?;
    drop(file);
    let system =
        PathBuf::from(std::env::var_os("SystemRoot").context("signature_verification_failed")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = Command::new(system);
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
        .arg(&script)
        .arg("-Installer")
        .arg(path)
        .arg("-Thumbprint")
        .arg(thumbprint)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let status = command.status().context("signature_verification_failed")?;
    let _ = std::fs::remove_file(script);
    if !status.success() {
        bail!("signature_verification_failed");
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct Recovery {
    version: String,
    schema: u32,
    database_sha256: String,
    files: Vec<(String, String)>,
}
fn hash(path: &Path) -> Result<String> {
    let f = File::open(path)?;
    platform::hash_extent(&f, 0, f.metadata()?.len())
}
pub fn snapshot(profile: &Path, installed: &Path) -> Result<PathBuf> {
    let mut locks = Vec::new();
    for name in ["maintenance.lock", "engine.lock", "scheduler.lock"] {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(profile.join(name))?;
        fs2::FileExt::try_lock_exclusive(&lock).context("update_requires_idle_app")?;
        locks.push(lock);
    }
    let db = rusqlite::Connection::open(profile.join("jobs.db"))?;
    let schema: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if schema > 6 || integrity != "ok" {
        bail!("update_backup_failed");
    }
    let directory = profile
        .join("updates")
        .join(format!("recovery-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(directory.join("previous/media"))?;
    let backup = directory.join("jobs.sqlite");
    db.execute(
        "VACUUM INTO ?1",
        [backup.to_str().context("non_unicode_path")?],
    )?;
    let mut files = Vec::new();
    for name in [
        "idmxl-desktop.exe",
        "idmxl.exe",
        "media/ffmpeg.exe",
        "media/ffprobe.exe",
    ] {
        let from = installed.join(name);
        if !from.is_file() {
            bail!("update_backup_failed");
        }
        if platform::is_reparse(&from)? {
            bail!("update_backup_failed");
        }
        let to = directory.join("previous").join(name);
        std::fs::copy(&from, &to)?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&to)?
            .sync_all()?;
        files.push((name.into(), hash(&to)?));
    }
    let receipt = Recovery {
        version: env!("CARGO_PKG_VERSION").into(),
        schema,
        database_sha256: hash(&backup)?,
        files,
    };
    std::fs::write(
        directory.join("recovery.receipt"),
        platform::protect(&serde_json::to_vec(&receipt)?)?,
    )?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("recovery.receipt"))?
        .sync_all()?;
    Ok(directory)
}
pub fn restore(directory: &Path) -> Result<PathBuf> {
    let directory = directory.canonicalize()?;
    let bytes = std::fs::read(directory.join("recovery.receipt"))?;
    if bytes.len() > 65536 {
        bail!("invalid_update_receipt");
    }
    let receipt: Recovery =
        serde_json::from_slice(&platform::unprotect(&bytes)?).context("invalid_update_receipt")?;
    if receipt.schema > 6
        || receipt.files.len() != 4
        || hash(&directory.join("jobs.sqlite"))? != receipt.database_sha256
    {
        bail!("update_backup_changed");
    }
    for (name, digest) in &receipt.files {
        if ![
            "idmxl-desktop.exe",
            "idmxl.exe",
            "media/ffmpeg.exe",
            "media/ffprobe.exe",
        ]
        .contains(&name.as_str())
            || hash(&directory.join("previous").join(name))? != *digest
        {
            bail!("update_backup_changed");
        }
    }
    let db = rusqlite::Connection::open_with_flags(
        directory.join("jobs.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    if db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? != receipt.schema
        || db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? != "ok"
    {
        bail!("update_backup_changed");
    }
    let destination = directory.join(format!("restored-profile-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&destination)?;
    std::fs::copy(directory.join("jobs.sqlite"), destination.join("jobs.db"))?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(destination.join("jobs.db"))?
        .sync_all()?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn fixture() -> (ReleaseConfig, Manifest, SigningKey) {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        (
            ReleaseConfig {
                endpoint: "https://updates.example.org/stable.json".into(),
                artifact_origin: "https://updates.example.org".into(),
                public_key: STANDARD.encode(key.verifying_key().to_bytes()),
                certificate_thumbprint: "a".repeat(40),
                channel: "stable".into(),
            },
            Manifest {
                product: "dev.idmxl.desktop".into(),
                target: "windows-x86_64".into(),
                channel: "stable".into(),
                version: "0.2.0".into(),
                sequence: 10,
                issued: 1000000,
                expires: 2000000,
                url: "https://updates.example.org/setup.exe".into(),
                sha256: "a".repeat(64),
                size: 123,
                notes: "An update".into(),
            },
            key,
        )
    }
    fn signed(m: &Manifest, key: &SigningKey) -> Vec<u8> {
        let payload = serde_json::to_vec(m).unwrap();
        serde_json::to_vec(&Envelope {
            signature: STANDARD.encode(key.sign(&payload).to_bytes()),
            payload: STANDARD.encode(payload),
        })
        .unwrap()
    }
    #[test]
    fn signed_metadata_binds_identity_target_hash_and_freshness() {
        let (config, m, key) = fixture();
        let bytes = signed(&m, &key);
        assert_eq!(
            verify(&config, &bytes, "0.1.0", 1500000, &Watermark::default())
                .unwrap()
                .version,
            "0.2.0"
        );
        assert!(verify(&config, &bytes, "0.1.0", 2100000, &Watermark::default()).is_err());
        assert!(
            verify(
                &config,
                &signed(&m, &SigningKey::from_bytes(&[8u8; 32])),
                "0.1.0",
                1500000,
                &Watermark::default()
            )
            .is_err()
        );
        for change in 0..6 {
            let mut altered = m.clone();
            match change {
                0 => altered.target = "linux-x86_64".into(),
                1 => altered.product = "other".into(),
                2 => altered.url = "https://attacker.example.org/setup.exe".into(),
                3 => altered.expires = m.issued + 15 * 86_400_000,
                4 => altered.version = "0.1.0-beta.1".into(),
                _ => altered.sequence = 0,
            };
            assert!(
                verify(
                    &config,
                    &signed(&altered, &key),
                    "0.1.0",
                    1500000,
                    &Watermark::default()
                )
                .is_err()
            );
        }
    }
    #[test]
    fn replay_equivocation_and_downgrade_are_rejected() {
        let (config, m, key) = fixture();
        let payload = serde_json::to_vec(&m).unwrap();
        let watermark = Watermark {
            sequence: 10,
            issued: m.issued,
            payload_hash: format!("{:x}", Sha256::digest(payload)),
        };
        assert!(verify(&config, &signed(&m, &key), "0.1.0", 1500000, &watermark).is_ok());
        let mut altered = m.clone();
        altered.sequence = 9;
        assert!(
            verify(
                &config,
                &signed(&altered, &key),
                "0.1.0",
                1500000,
                &watermark
            )
            .is_err()
        );
        altered.sequence = 10;
        altered.notes = "different release under same sequence".into();
        assert!(
            verify(
                &config,
                &signed(&altered, &key),
                "0.1.0",
                1500000,
                &watermark
            )
            .is_err()
        );
        assert!(verify(&config, &signed(&m, &key), "0.3.0", 1500000, &watermark).is_err());
    }
    #[test]
    fn paired_recovery_preserves_current_profile_and_rejects_changed_backups() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        let installed = root.path().join("installed");
        std::fs::create_dir_all(installed.join("media")).unwrap();
        let store = Store::open(&profile).unwrap();
        store
            .add(AddJob {
                url: "https://example.org/file".into(),
                destination: root.path().join("file.bin"),
                replayable: false,
                expected_sha256: None,
            })
            .unwrap();
        for name in [
            "idmxl-desktop.exe",
            "idmxl.exe",
            "media/ffmpeg.exe",
            "media/ffprobe.exe",
        ] {
            std::fs::write(installed.join(name), name).unwrap();
        }
        assert!(snapshot(&profile, &installed).is_err());
        drop(store);
        let backup = snapshot(&profile, &installed).unwrap();
        let current = hash(&profile.join("jobs.db")).unwrap();
        let restored = restore(&backup).unwrap();
        assert_eq!(hash(&profile.join("jobs.db")).unwrap(), current);
        assert_eq!(Store::open(&restored).unwrap().list().unwrap().len(), 1);
        std::fs::write(backup.join("previous/idmxl.exe"), b"tampered").unwrap();
        assert!(restore(&backup).is_err());
    }
}
