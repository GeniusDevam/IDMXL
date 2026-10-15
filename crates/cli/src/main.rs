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
        #[arg(long,default_value_t=2,value_parser=clap::value_parser!(u8).range(1..=4))]
        connections: u8,
        #[arg(long,default_value_t=2,value_parser=clap::value_parser!(u8).range(0..=5))]
        retries: u8,
        #[arg(
            long,
            default_value_t = 0,
            help = "Aggregate bytes per second for this job; zero is unlimited"
        )]
        speed_limit: u64,
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
                    stop,
                },
            )?)?
        }
        Command::List => serde_json::to_value(store.list()?)?,
        Command::Show { id } => serde_json::to_value(store.view(&id)?)?,
        Command::Pause { id } => serde_json::to_value(store.request(&id, "paused")?)?,
        Command::Cancel { id } => serde_json::to_value(store.request(&id, "canceled")?)?,
        Command::Doctor => unreachable!(),
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        // Error contexts are controlled codes. Do not dump request objects, URLs or headers.
        eprintln!("{}", serde_json::json!({"error":error.to_string()}));
        std::process::exit(1);
    }
}
