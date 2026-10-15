# IDMXL

A Windows download manager in early development. The first milestone is a real Rust/libcurl file-transfer engine with durable SQLite checkpoints. The adopted design is in [docs/PLAN.md](docs/PLAN.md); completed work and remaining gates are in [PROGRESS.md](PROGRESS.md).

## Current capabilities

- HTTP(S) file downloads, with public file URLs or DPAPI-protected signed URLs on Windows.
- Single-request mode by default. An explicit `--replayable` option enables probing, strong-validator checks and up to four concurrent byte ranges.
- Durable 1 MiB extents, pause/resume, process-crash recovery, and verification of saved extents before reuse.
- Bounded retries for eligible range-transfer failures, numeric Retry-After handling, and optional per-job speed limiting.
- SHA-256 validation, no-overwrite publication, recovery across rename/database boundaries, and Internet-zone attachment metadata on Windows.
- Persistent, redacted JSON history; a single active engine per state directory.
- An original Tauri/React Windows interface with add/start/pause/cancel, search, summary counts and hash details. Downloads run in a separate CLI process and history survives closing/reopening the window.

This is a development milestone, not a completed Core release. Adaptive tuning, automatic multi-job scheduling, browser handoff, media acquisition, installation and signed updates are not yet implemented. NTFS is the intended first production filesystem; real power-loss and clean-machine qualification are pending. Fixed parallelism has not been benchmarked as faster than a browser.

## Open the desktop preview

For the already-built local copy, double-click `Start-IDMXL.cmd`. The preview uses the generated test history from native QA; the generated test file is under `.local/` and is not committed to Git.

To build from a fresh clone, also install Node.js 20.19+ or 22.12+ and a supported WebView2 Runtime, then run:

```powershell
.\scripts\build.ps1
.\scripts\start.ps1
```

`scripts/build.ps1 -Release` builds a release preview. `-Bundle` is a future installer-development path; its NSIS output, signing, clean-machine behavior and upgrade/uninstall contract are not yet qualified. The development launcher works after building in the current checkout. Release builds place the engine alongside the app; source control excludes generated binaries and frontend output.

In the app, add an HTTP(S) file URL, choose an existing destination folder, optionally enable reusable-link probing or enter a trusted digest, then press Start on its row. One job runs at a time. Progress currently shows durable checkpoint bytes, not unflushed in-flight bytes; sequential jobs may show no byte progress until their full request is checkpointed. Leave probing disabled for uncertain one-use links.

## Build

Prerequisites: Windows 11 x64, Microsoft C++ Build Tools and Windows SDK, and Rust/rustup. The repository pins Rust 1.99.0. Cargo builds libcurl, nghttp2 and SQLite from source; no Python runtime is needed by the application.

```powershell
cargo build --workspace --locked
cargo test --workspace --locked
python -m unittest discover -s tests -v
.\target\debug\idmxl.exe doctor
```

Python 3.11+ is used only for deterministic end-to-end tests. They run a temporary HTTP server on loopback and use generated files. Network-restricted execution sandboxes must permit loopback for the test processes.

## Use the CLI

```powershell
# Queue a reusable file URL and record a trusted digest if one is available.
.\target\debug\idmxl.exe add https://example.org/file.zip --output C:\Downloads\file.zip --replayable

# Run the returned job ID. Ctrl+C pauses it.
.\target\debug\idmxl.exe run <job-id> --connections 2
.\target\debug\idmxl.exe list
.\target\debug\idmxl.exe pause <job-id>
.\target\debug\idmxl.exe run <job-id>
```

The destination folder must already exist. Existing files are never silently replaced. Job data defaults to `%LOCALAPPDATA%\IDMXL`; use `--state-dir <folder>` for an isolated profile.

Use `add --url-stdin --output <path>` for URLs with secret tokens so those tokens are not placed in process arguments. On Windows, original and resolved URLs are protected with user-scoped DPAPI; the UI/history display removes query strings and fragments. Do not include credentials or tokens in the URL path. Password-bearing URLs are rejected. DPAPI does not protect against malicious code already running as the same user.

Without `--replayable`, the engine skips probing and uses a single request. Interrupted sequential jobs restart safely. With `--replayable`, safe resume requires matching strong identity and a valid range response. Missing/weak validators cause sequential fallback. Refreshing a signed URL is not implemented yet.

Cancel preserves partial files and prevents further running of that job. The current CLI intentionally does not delete downloads or partials. Only one job runs at a time per profile; a second engine receives `engine_busy`.

## Repository layout

- `crates/engine`: transport, identity validation, positional writes, SQLite journal and publication recovery.
- `crates/cli`: JSON command-line interface and interrupt handling.
- `tests`: deterministic HTTP integration fixtures and failure scenarios.
- `apps/desktop`: React interface, restrictive Tauri capabilities and native commands.
- `scripts`: build, launch and validation helpers. They prefer normal installed tools and can also use this workspace's local Rust install without global PATH changes.
- `docs`: adopted plan, architecture decisions, test evidence and dependency policy.

## Development policy

Keep secrets out of logs and command arguments. Keep the engine independent of desktop windows. Do not loosen range/identity checks to make a site appear supported. Add browser and media capabilities only with explicit supported subsets and fixture evidence. Every milestone must update `PROGRESS.md` and the feature matrix.

No licence for the original IDM code or branding is implied. Dependency notices and distribution materials must be completed before any public binary release. The repository's product licence has not yet been selected.

