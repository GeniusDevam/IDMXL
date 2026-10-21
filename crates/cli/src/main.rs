use std::{
    io::Read,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use idmxl_engine::{AddJob, Engine, RunOptions, Store};
mod native_host;

#[derive(Parser)]
#[command(name = "idmxl", version, about = "IDMXL development download engine")]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Job database folder; defaults to LOCALAPPDATA/IDMXL"
    )]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Queue an HTTP(S) file. URLs with secret tokens should use --url-stdin.
    Add {
        #[arg(required_unless_present = "url_stdin", conflicts_with = "url_stdin")]
        url: Option<String>,
        #[arg(long)]
        url_stdin: bool,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, help = "Permit probing and repeated conditional range requests")]
        replayable: bool,
        #[arg(long)]
        sha256: Option<String>,
    },
    /// Run or safely resume one queued job. Ctrl+C pauses it.
    Run {
        id: String,
        #[arg(long,default_value_t=2,value_parser=clap::value_parser!(u8).range(1..=8))]
        connections: u8,
        #[arg(long,default_value_t=2,value_parser=clap::value_parser!(u8).range(0..=5))]
        retries: u8,
        #[arg(
            long,
            default_value_t = 0,
            help = "Aggregate bytes per second for this job; zero is unlimited"
        )]
        speed_limit: u64,
        #[arg(
            long,
            help = "Experimentally tune 1..connections for uncapped conditional ranges"
        )]
        adaptive: bool,
        #[arg(
            long,
            help = "Write numeric performance diagnostics to a new JSON file"
        )]
        metrics: Option<PathBuf>,
        #[arg(long,value_parser=clap::value_parser!(u8).range(1..=32),help="Experimental HTTP range span in MiB; recovery blocks stay 1 MiB")]
        request_mib: Option<u8>,
        #[arg(long,value_parser=["auto","http1","http2"],default_value="auto",help="HTTP version preference for body requests")]
        http_version: String,
    },
    /// List the newest 1,000 jobs as redacted JSON.
    List,
    /// Show a job without revealing its protected source URL.
    Show { id: String },
    /// Ask a running engine to stop; partial data is preserved.
    Pause { id: String },
    /// Cancel a job without deleting any user files.
    Cancel { id: String },
    /// Show the capabilities of the actually linked transport.
    Doctor,
    /// Queue up to 1,000 explicit file jobs from a JSON array on stdin. Never starts them.
    Batch,
    /// Add a plain link list with automatic filenames and optional category folders from JSON stdin.
    AddLinks,
    /// Inspect a direct media, HLS VOD or static DASH URL from JSON stdin.
    MediaInspect,
    /// Queue selected original tracks and a local stream-copy output from JSON stdin.
    MediaAdd,
    /// Show the chosen media tracks without revealing their URLs.
    MediaDetails { id: String },
    /// Replace an expired file URL from JSON stdin, retaining only provably matching checkpoints.
    RefreshSource { id: String },
    /// Check authenticated release metadata; unconfigured previews do not access the network.
    UpdateCheck,
    /// Restore an update backup into a separate profile, preserving the current profile.
    UpdateRecover { directory: PathBuf },
    /// Get a queue's recurring local-time hours; --set reads a window or null from stdin.
    QueueWindow {
        queue: String,
        #[arg(long)]
        set: bool,
    },
    /// Get saved download-folder rules; --set reads folder settings from JSON stdin.
    Folders {
        #[arg(long)]
        set: bool,
    },
    /// Persist a job's queue, priority, UTC schedule and transfer options from JSON stdin.
    Policy { id: String },
    /// Run eligible jobs in priority/FIFO order. --watch waits for future schedules.
    QueueRun {
        #[arg(long)]
        watch: bool,
        #[arg(long, hide = true)]
        startup_id: Option<uuid::Uuid>,
    },
    /// Stop queue admission and pause its current transfer.
    QueuePause,
    /// Set the global queue bandwidth ceiling in bytes per second; zero means unlimited.
    QueueLimit { bytes_per_second: u64 },
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Command::Doctor) {
        println!(
            "{}",
            serde_json::to_string_pretty(&idmxl_engine::capabilities())?
        );
        return Ok(());
    }
    let directory = cli.state_dir.unwrap_or_else(|| {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".local"))
            .join("IDMXL")
    });
    if let Command::UpdateCheck = cli.command {
        println!(
            "{}",
            serde_json::to_string_pretty(&idmxl_engine::updates::check(&directory)?)?
        );
        return Ok(());
    }
    if let Command::UpdateRecover { directory } = &cli.command {
        println!(
            "{}",
            serde_json::json!({"recovered_profile":idmxl_engine::updates::restore(directory)?})
        );
        return Ok(());
    }
    let store = Store::open(&directory)?;
    let value = match cli.command {
        Command::Add {
            url,
            url_stdin,
            output,
            replayable,
            sha256,
        } => {
            let url = if url_stdin {
                let mut text = String::new();
                std::io::stdin().take(65537).read_to_string(&mut text)?;
                if text.len() > 65536 {
                    bail!("input_too_large");
                }
                text.trim().to_owned()
            } else {
                url.context("url_required")?
            };
            serde_json::to_value(store.add(AddJob {
                url,
                destination: output,
                replayable,
                expected_sha256: sha256,
            })?)?
        }
        Command::Run {
            id,
            connections,
            retries,
            speed_limit,
            adaptive,
            metrics,
            request_mib,
            http_version,
        } => {
            let stop = Arc::new(AtomicBool::new(false));
            let signal = stop.clone();
            ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
            serde_json::to_value(Engine::new(store).run(
                &id,
                RunOptions {
                    connections: connections as usize,
                    retries: retries as u32,
                    speed_limit_bytes: speed_limit,
                    adaptive,
                    metrics,
                    request_mib,
                    http_version: match http_version.as_str() {
                        "http1" => Some(curl::easy::HttpVersion::V11),
                        "http2" => Some(curl::easy::HttpVersion::V2TLS),
                        _ => None,
                    },
                    stop,
                },
            )?)?
        }
        Command::RefreshSource { id } => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                url: String,
            }
            let request: Input = read_json(20000)?;
            serde_json::to_value(store.refresh_source(&id, &request.url)?)?
        }
        Command::List => serde_json::to_value(store.list()?)?,
        Command::Show { id } => serde_json::to_value(store.view(&id)?)?,
        Command::Pause { id } => serde_json::to_value(store.request(&id, "paused")?)?,
        Command::Cancel { id } => serde_json::to_value(store.request(&id, "canceled")?)?,
        Command::Doctor => unreachable!(),
        Command::UpdateCheck | Command::UpdateRecover { .. } => unreachable!(),
        Command::QueueWindow { queue, set } => {
            if set {
                let window: Option<idmxl_engine::QueueWindow> = read_json(16384)?;
                store.set_queue_window(&queue, window.as_ref())?;
            }
            serde_json::json!({"window":store.queue_window(&queue)?,"open_now":store.queue_open(&queue)?})
        }
        Command::Folders { set } => {
            if set {
                let settings: idmxl_engine::FolderSettings = read_json(65536)?;
                store.set_folder_settings(&settings)?;
            }
            serde_json::json!({"folders":store.folder_settings()?,"default_rules":idmxl_engine::workflows::default_folder_rules()})
        }
        Command::AddLinks => {
            let batch: idmxl_engine::LinkBatch = read_json(1048576)?;
            serde_json::json!({"results":store.add_links(batch)?})
        }
        Command::MediaInspect => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Source {
                url: String,
            }
            let input: Source = read_json(65536)?;
            serde_json::to_value(idmxl_engine::manifests::discover(&input.url, false)?.view())?
        }
        Command::MediaAdd => serde_json::to_value(store.add_media(read_json(65536)?, false)?)?,
        Command::MediaDetails { id } => serde_json::to_value(store.media_details(&id)?)?,
        Command::Policy { id } => {
            let mut text = String::new();
            std::io::stdin().take(16385).read_to_string(&mut text)?;
            if text.len() > 16384 {
                bail!("input_too_large");
            }
            let policy = serde_json::from_str(&text).context("invalid_queue_policy")?;
            serde_json::to_value(store.set_policy(&id, &policy)?)?
        }
        Command::QueueRun { watch, startup_id } => {
            let stop = Arc::new(AtomicBool::new(false));
            let signal = stop.clone();
            ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
            if startup_id.is_none() {
                store.set_queue_running(true)?;
            }
            let ready_path =
                startup_id.map(|id| store.directory.join(format!(".worker-ready-{id}")));
            let result =
                idmxl_engine::scheduler::drain_notifying(&Engine::new(store), watch, stop, || {
                    if let Some(path) = &ready_path {
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(path)?
                            .sync_all()?;
                    }
                    Ok(())
                });
            if let Some(path) = ready_path {
                let _ = std::fs::remove_file(path);
            }
            result?;
            serde_json::json!({"status":"queue_worker_stopped"})
        }
        Command::QueuePause => {
            store.set_queue_running(false)?;
            serde_json::json!({"status":"queue_paused"})
        }
        Command::QueueLimit { bytes_per_second } => {
            store.set_global_speed_limit(bytes_per_second)?;
            serde_json::json!({"bytes_per_second":bytes_per_second.to_string()})
        }
        Command::Batch => {
            let mut text = String::new();
            std::io::stdin().take(1048577).read_to_string(&mut text)?;
            if text.len() > 1048576 {
                bail!("input_too_large");
            }
            let requests: Vec<AddJob> = serde_json::from_str(&text).context("invalid_batch")?;
            if requests.is_empty() || requests.len() > 1000 {
                bail!("invalid_batch_size");
            }
            let mut seen = std::collections::HashSet::new();
            let mut results = Vec::new();
            for (index, request) in requests.into_iter().enumerate() {
                if !seen.insert((request.url.clone(), request.destination.clone())) {
                    results.push(serde_json::json!({"index":index,"status":"duplicate"}));
                    continue;
                }
                match store.add(request) {
                    Ok(job)=>results.push(serde_json::json!({"index":index,"status":"queued","job":job})),
                    Err(_)=>results.push(serde_json::json!({"index":index,"status":"rejected","error":"invalid_or_conflicting_job"})),
                }
            }
            serde_json::json!({"results":results})
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(limit: u64) -> Result<T> {
    let mut text = String::new();
    std::io::stdin().take(limit + 1).read_to_string(&mut text)?;
    if text.len() as u64 > limit {
        bail!("input_too_large");
    }
    serde_json::from_str(&text).context("invalid_input")
}

fn main() {
    let first = std::env::args().nth(1).unwrap_or_default();
    let result = if first.starts_with("chrome-extension://") {
        native_host::serve(&first)
    } else {
        run()
    };
    if let Err(error) = result {
        // Error contexts are controlled codes. Do not dump request objects, URLs or headers.
        eprintln!("{}", serde_json::json!({"error":error.to_string()}));
        std::process::exit(1);
    }
}
