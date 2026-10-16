use crate::{
    AddJob, Engine, JobView, RunOptions, Store,
    manifests::{self, Piece, Track},
    platform,
};
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaRequest {
    pub url: String,
    pub selection: String,
    pub audio_selection: Option<String>,
    pub audio_url: Option<String>,
    pub destination: PathBuf,
    pub container: String,
    #[serde(default)]
    pub audio_only: bool,
}
#[derive(Serialize, Deserialize)]
struct SavedPiece {
    job: String,
    range: Option<(u64, u64)>,
}
#[derive(Serialize, Deserialize)]
struct SavedTrack {
    role: String,
    flavor: String,
    pieces: Vec<SavedPiece>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<f64>,
}
#[derive(Serialize, Deserialize)]
struct Plan {
    version: u8,
    workspace: PathBuf,
    container: String,
    audio_only: bool,
    tracks: Vec<SavedTrack>,
    request_fingerprint: String,
}
impl Store {
    pub fn add_media(&self, request: MediaRequest, public_only: bool) -> Result<JobView> {
        self.add_media_inner(request, public_only, None)
    }
    pub fn add_media_handoff(&self, request: MediaRequest, request_id: &str) -> Result<JobView> {
        uuid::Uuid::parse_str(request_id).context("invalid_request_id")?;
        self.add_media_inner(request, true, Some(request_id))
    }
    fn add_media_inner(
        &self,
        request: MediaRequest,
        public_only: bool,
        handoff: Option<&str>,
    ) -> Result<JobView> {
        use sha2::{Digest, Sha256};
        let request_fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&request)?));
        if let Some(handoff) = handoff {
            let existing: Option<String> = self
                .db
                .query_row(
                    "SELECT job_id FROM handoffs WHERE request_id=?1",
                    [handoff],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(id) = existing {
                let bytes: Vec<u8> = self
                    .db
                    .query_row("SELECT plan FROM media_jobs WHERE job_id=?1", [&id], |r| {
                        r.get(0)
                    })
                    .context("media_preparation_incomplete")?;
                let plan: Plan = serde_json::from_slice(&platform::unprotect(&bytes)?)?;
                if plan.request_fingerprint != request_fingerprint {
                    bail!("handoff_id_conflict");
                }
                return self.view(&id);
            }
        }
        crate::media_helper::paths()?;
        let muxer = muxer(&request.container)?;
        let extension = request
            .destination
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension != request.container {
            bail!("media_extension_mismatch");
        }
        let discovery = manifests::discover(&request.url, public_only)?;
        let selected = discovery
            .choices
            .iter()
            .find(|c| c.id == request.selection)
            .context("media_selection_changed")?;
        let mut tracks = Vec::new();
        let to_track = |choice: &manifests::Choice| -> Result<Track> {
            if let Some(track) = &choice.track {
                return Ok(track.clone());
            }
            if discovery.kind == "hls" {
                let variant = manifests::discover(&choice.url, public_only)?;
                let mut track = variant
                    .choices
                    .first()
                    .and_then(|c| c.track.clone())
                    .context("unsupported_media_nesting")?;
                track.role = if choice.role == "video"
                    && !discovery
                        .choices
                        .iter()
                        .any(|c| c.role == "audio" && c.group == choice.group)
                {
                    "muxed".into()
                } else {
                    choice.role.clone()
                };
                track.width = choice.width;
                track.height = choice.height;
                Ok(track)
            } else {
                Ok(Track {
                    role: choice.role.clone(),
                    pieces: vec![Piece {
                        url: choice.url.clone(),
                        range: None,
                    }],
                    width: choice.width,
                    height: choice.height,
                    duration: None,
                })
            }
        };
        tracks.push(to_track(selected)?);
        if let Some(audio) = request.audio_selection.as_ref().filter(|s| !s.is_empty()) {
            let audio = discovery
                .choices
                .iter()
                .find(|c| &c.id == audio && c.role == "audio")
                .context("media_selection_changed")?;
            if discovery.kind == "hls" && audio.group != selected.group {
                bail!("media_audio_group_mismatch");
            }
            if selected.role == "audio" {
                bail!("invalid_media_selection");
            }
            tracks.push(to_track(audio)?);
        } else if selected.group.is_some()
            && discovery
                .choices
                .iter()
                .any(|c| c.role == "audio" && c.group == selected.group)
        {
            bail!("media_audio_selection_required");
        }
        if let Some(audio) = request.audio_url.filter(|s| !s.is_empty()) {
            if discovery.kind != "direct" || tracks.len() != 1 || request.audio_only {
                bail!("invalid_media_selection");
            }
            platform::checked_url(&audio)?;
            tracks[0].role = "video".into();
            tracks.push(Track {
                role: "audio".into(),
                pieces: vec![Piece {
                    url: audio,
                    range: None,
                }],
                width: None,
                height: None,
                duration: None,
            });
        }
        if request.audio_only && tracks.iter().any(|t| t.role == "audio") {
            tracks.retain(|t| t.role == "audio");
        }
        if tracks.len() > 2 || tracks.iter().map(|t| t.pieces.len()).sum::<usize>() > 20_000 {
            bail!("media_too_many_segments");
        }
        let parent = self.insert(
            AddJob {
                url: request.url,
                destination: request.destination,
                replayable: false,
                expected_sha256: None,
            },
            handoff,
            false,
            "Default",
            None,
            Some("preparing_media"),
        )?;
        let job = self.get(&parent.id)?;
        let workspace = job
            .destination
            .parent()
            .context("invalid_destination")?
            .join(format!(".idmxl-media-{}", parent.id));
        std::fs::create_dir(&workspace).context("media_workspace_changed")?;
        let mut sources: HashMap<String, String> = HashMap::new();
        let mut saved = Vec::new();
        for track in tracks {
            let mut pieces = Vec::new();
            for piece in track.pieces {
                let child = if let Some(id) = sources.get(&piece.url) {
                    id.clone()
                } else {
                    let component = self.insert(
                        AddJob {
                            url: piece.url.clone(),
                            destination: workspace.join(format!("source-{}.media", sources.len())),
                            replayable: discovery.kind != "direct",
                            expected_sha256: None,
                        },
                        None,
                        false,
                        "Default",
                        None,
                        Some("media_component"),
                    )?;
                    self.db.execute(
                        "INSERT INTO media_components VALUES(?1,?2)",
                        params![component.id, parent.id],
                    )?;
                    if public_only {
                        self.db.execute(
                            "UPDATE jobs SET public_only=1 WHERE id=?1",
                            [&component.id],
                        )?;
                    }
                    sources.insert(piece.url, component.id.clone());
                    component.id
                };
                pieces.push(SavedPiece {
                    job: child,
                    range: piece.range,
                });
            }
            saved.push(SavedTrack {
                role: track.role,
                flavor: discovery.kind.clone(),
                pieces,
                width: track.width,
                height: track.height,
                duration: track.duration,
            });
        }
        let plan = Plan {
            version: 1,
            workspace,
            container: muxer.into(),
            audio_only: request.audio_only,
            tracks: saved,
            request_fingerprint,
        };
        let encoded = serde_json::to_vec(&plan)?;
        if encoded.len() > 8 * 1024 * 1024 {
            bail!("media_too_many_segments");
        }
        let tx = self.db.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO media_jobs VALUES(?1,?2)",
            params![parent.id, platform::protect(&encoded)?],
        )?;
        tx.execute(
            "UPDATE jobs SET state='queued',requested='run',mode='media' WHERE id=?1",
            [&parent.id],
        )?;
        tx.commit()?;
        self.view(&parent.id)
    }
    pub(crate) fn is_media(&self, id: &str) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM media_jobs WHERE job_id=?1)",
            [id],
            |r| r.get(0),
        )?)
    }
    pub fn media_details(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let encoded: Option<Vec<u8>> = self
            .db
            .query_row("SELECT plan FROM media_jobs WHERE job_id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(encoded) = encoded else {
            return Ok(None);
        };
        let plan: Plan = serde_json::from_slice(&platform::unprotect(&encoded)?)?;
        Ok(Some(
            serde_json::json!({"container":plan.container,"audio_only":plan.audio_only,"tracks":plan.tracks.iter().map(|t|serde_json::json!({"role":t.role,"width":t.width,"height":t.height,"segments":t.pieces.len(),"duration":t.duration})).collect::<Vec<_>>()}),
        ))
    }
}
fn muxer(container: &str) -> Result<&str> {
    match container {
        "mp4" => Ok("mp4"),
        "m4a" => Ok("mp4"),
        "mkv" => Ok("matroska"),
        "webm" => Ok("webm"),
        "mov" => Ok("mov"),
        "mp3" => Ok("mp3"),
        "wav" => Ok("wav"),
        "ogg" => Ok("ogg"),
        _ => bail!("unsupported_media_container"),
    }
}
fn stopped(engine: &Engine, id: &str, options: &RunOptions) -> Result<bool> {
    Ok(options.stop.load(Ordering::Relaxed) || engine.store.requested(id)? != "run")
}
fn checked_workspace(path: &std::path::Path) -> Result<()> {
    if !path.is_dir() || platform::is_reparse(path)? {
        bail!("media_workspace_changed");
    }
    Ok(())
}
pub(crate) fn execute(engine: &Engine, id: &str, options: &RunOptions) -> Result<()> {
    let done = Arc::new(AtomicBool::new(false));
    let local_stop = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let signal = local_stop.clone();
    let external = options.stop.clone();
    let directory = engine.store.directory.clone();
    let parent = id.to_owned();
    let monitor = std::thread::spawn(move || {
        if let Ok(store) = Store::open(&directory) {
            while !finished.load(Ordering::Relaxed) {
                if external.load(Ordering::Relaxed)
                    || !store.requested(&parent).is_ok_and(|s| s == "run")
                {
                    signal.store(true, Ordering::Relaxed);
                    break;
                }
                let _=store.db.execute("UPDATE jobs SET durable=(SELECT COALESCE(SUM(durable),0) FROM jobs WHERE id IN (SELECT job_id FROM media_components WHERE parent_id=?1)),received=(SELECT COALESCE(SUM(received),0) FROM jobs WHERE id IN (SELECT job_id FROM media_components WHERE parent_id=?1)),speed=(SELECT COALESCE(SUM(speed),0) FROM jobs WHERE id IN (SELECT job_id FROM media_components WHERE parent_id=?1)) WHERE id=?1 AND state IN ('downloading','verifying')",[&parent]);
                std::thread::sleep(Duration::from_millis(100));
            }
        } else {
            signal.store(true, Ordering::Relaxed);
        }
    });
    let result = execute_inner(
        engine,
        id,
        &RunOptions {
            stop: local_stop,
            ..options.clone()
        },
    );
    done.store(true, Ordering::Relaxed);
    let _ = monitor.join();
    result
}
fn execute_inner(engine: &Engine, id: &str, options: &RunOptions) -> Result<()> {
    let bytes: Vec<u8> =
        engine
            .store
            .db
            .query_row("SELECT plan FROM media_jobs WHERE job_id=?1", [id], |r| {
                r.get(0)
            })?;
    let plan: Plan = serde_json::from_slice(&platform::unprotect(&bytes)?)?;
    if plan.version != 1 {
        bail!("unsupported_media_plan");
    }
    checked_workspace(&plan.workspace)?;
    let components: Vec<String> = {
        let mut q = engine
            .store
            .db
            .prepare("SELECT job_id FROM media_components WHERE parent_id=?1 ORDER BY rowid")?;
        q.query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for component in &components {
        if stopped(engine, id, options)? {
            bail!("transfer_interrupted");
        }
        if engine.store.get(component)?.state == "completed" {
            continue;
        }
        let result = engine.run_locked(component, options.clone());
        if stopped(engine, id, options)? {
            bail!("transfer_interrupted");
        }
        if result.is_err() || engine.store.get(component)?.state != "completed" {
            bail!("media_component_failed");
        }
    }
    let needed = components
        .iter()
        .map(|id| engine.store.get(id).map(|j| j.total.unwrap_or(0)))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .try_fold(0u64, |sum, n| sum.checked_add(n))
        .context("file_too_large")?;
    if fs2::available_space(&plan.workspace)?
        < needed.saturating_mul(2).saturating_add(16 * 1024 * 1024)
    {
        bail!("insufficient_disk_space");
    }
    engine.store.state(id, "verifying", None)?;
    let (ffmpeg, ffprobe) = crate::media_helper::paths()?;
    crate::fault::point("before_media_assembly");
    let mut verified = HashMap::new();
    for component in &components {
        let source = engine.store.get(component)?;
        if !source.destination.starts_with(&plan.workspace)
            || platform::is_reparse(&source.destination)?
        {
            bail!("media_workspace_changed");
        }
        let mut open = OpenOptions::new();
        open.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            open.share_mode(1).custom_flags(0x00200000);
        }
        let file = open.open(&source.destination)?;
        let size = file.metadata()?.len();
        if source.actual_sha256.as_deref() != Some(platform::hash_extent(&file, 0, size)?.as_str())
        {
            bail!("media_verification_failed");
        }
        verified.insert(component.clone(), file);
    }
    let mut input_files = Vec::new();
    let mut input_metadata = Vec::new();
    for (index, track) in plan.tracks.iter().enumerate() {
        checked_workspace(&plan.workspace)?;
        let name = format!("track-{index}-{}.media", uuid::Uuid::new_v4());
        let path = plan.workspace.join(&name);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        for piece in &track.pieces {
            if stopped(engine, id, options)? {
                bail!("transfer_interrupted");
            }
            let mut input = verified
                .get(&piece.job)
                .context("media_component_failed")?
                .try_clone()?;
            let size = input.metadata()?.len();
            let (offset, length) = piece.range.unwrap_or((0, size));
            if offset.checked_add(length).is_none_or(|end| end > size) {
                bail!("invalid_media_range");
            }
            input.seek(SeekFrom::Start(offset))?;
            let mut bounded = input.take(length);
            let mut buffer = [0u8; 65536];
            loop {
                if stopped(engine, id, options)? {
                    bail!("transfer_interrupted");
                }
                let count = bounded.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                output.write_all(&buffer[..count])?;
            }
        }
        output.sync_all()?;
        drop(output);
        crate::media_helper::validate_file_kind(&path, &track.flavor)?;
        let metadata =
            crate::media_helper::probe(&ffprobe, &plan.workspace, &name, options.stop.clone())?;
        crate::media_helper::validate_source(
            &metadata,
            &track.role,
            track.width,
            track.height,
            track.duration,
            plan.audio_only,
        )?;
        input_files.push(name);
        input_metadata.push(metadata);
    }
    let output_name = format!("output-{}.media", uuid::Uuid::new_v4());
    let roles: Vec<_> = plan.tracks.iter().map(|t| t.role.as_str()).collect();
    crate::media_helper::remux(
        &ffmpeg,
        &plan.workspace,
        &input_files,
        &output_name,
        crate::media_helper::RemuxSettings {
            container: &plan.container,
            audio_only: plan.audio_only,
            roles: &roles,
            aac_to_asc: crate::media_helper::uses_aac(&input_metadata, &roles),
        },
        options.stop.clone(),
    )?;
    let output_metadata = crate::media_helper::probe(
        &ffprobe,
        &plan.workspace,
        &output_name,
        options.stop.clone(),
    )?;
    crate::media_helper::validate_output(
        &input_metadata,
        &output_metadata,
        &plan
            .tracks
            .iter()
            .map(|t| t.role.as_str())
            .collect::<Vec<_>>(),
        plan.audio_only,
    )?;
    if stopped(engine, id, options)? {
        bail!("transfer_interrupted");
    }
    let parent = engine.store.get(id)?;
    let output_path = plan.workspace.join(output_name);
    if platform::is_reparse(&output_path)? {
        bail!("media_workspace_changed");
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&output_path)?;
    file.sync_all()?;
    let size = file.metadata()?.len();
    let digest = platform::hash_extent(&file, 0, size)?;
    let file_id = platform::file_id(&file)?;
    drop(file);
    // Persist the final owned file only through the ordinary recoverable publication protocol.
    let tx = engine.store.db.unchecked_transaction()?;
    tx.execute("UPDATE jobs SET staging=?2,state='publishing',total=?3,actual_sha256=?4,publication_file_id=?5 WHERE id=?1",params![id,output_path.to_str().context("non_unicode_path")?,size,digest,file_id])?;
    tx.commit()?;
    crate::fault::point("after_publication_intent");
    platform::mark_origin(&output_path)?;
    platform::publish(&output_path, &parent.destination)?;
    crate::fault::point("after_rename");
    engine.store.complete(id)
}
