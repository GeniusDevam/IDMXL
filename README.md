# IDMXL

A Windows download manager in early development. The first milestone is a real Rust/libcurl file-transfer engine with durable SQLite checkpoints. The adopted design is in [docs/PLAN.md](docs/PLAN.md); completed work and remaining gates are in [PROGRESS.md](PROGRESS.md).

## Current capabilities

- HTTP(S) file downloads, with public file URLs or DPAPI-protected signed URLs on Windows.
- Single-request mode by default. An explicit `--replayable` option enables probing, strong-validator checks and up to four concurrent byte ranges.
- Durable 1 MiB extents, pause/resume, process-crash recovery, and verification of saved extents before reuse.
- Bounded range retries, numeric/HTTP-date Retry-After and persisted origin cooldowns.
- Experimental CLI `run <id> --connections 4 --adaptive` tunes uncapped conditional ranges within the selected ceiling. Defaults and rate-capped jobs retain fixed admission.
- SHA-256 validation, no-overwrite publication, recovery across rename/database boundaries, and Internet-zone attachment metadata on Windows.
- Persistent, redacted JSON history, live received/durable bytes, speed and ETA.
- Automatic named queues, priorities, one-time UTC schedules/deadlines, recurring local-time weekday/overnight queue hours, checkpoint-based rotation, and per-job/global queue rate ceilings. One job runs at a time.
- Desktop plain-link import, editable file-type folder rules, automatic collision-safe names, and bounded/deduplicated JSON batches; explicit public-link browser handoff extension and durable native protocol.
- An original Tauri/React Windows interface with add/start/pause/cancel, search, schedules, tray, themes and reveal-in-folder. Detached engine processes keep downloads running when the window is hidden/closed.
- Idle-only schema migrations with pre-migration SQLite snapshots, a per-user NSIS installer build and dependency/notice inventory tooling.

This is a development milestone, not a completed Core release. Default/UI adaptive integration and broad benchmarks, wake scheduling, strict quotas, signed updates and the Media/Extended releases remain open. Browser installation/click-through and installer clean-machine/upgrade/uninstall qualification remain open. The controlled 8K MOV/PCM remux experiment is documented but not bundled as a media feature. The adaptive experiment uses an artificial localhost per-request bottleneck and establishes no general or browser speed advantage. See [PROGRESS.md](PROGRESS.md) for the full remaining gates.

## Open the desktop preview

For the already-built local copy, double-click `Start-IDMXL.cmd`. The preview uses the generated test history from native QA; the generated test file is under `.local/` and is not committed to Git.

To build from a fresh clone, also install Node.js 20.19+ or 22.12+ and a supported WebView2 Runtime, then run:

```powershell
.\scripts\build.ps1
.\scripts\start.ps1
```

`scripts/build.ps1 -Release` builds a release preview; `-Bundle` builds the per-user NSIS installer. The installer is unsigned. One isolated development-machine install/uninstall preservation smoke passed; clean-machine, upgrade and broader uninstall qualification remain open. The launcher prefers a built release and falls back to debug. Release builds place the engine alongside the app; generated binaries and frontend output are excluded from source control.

In the app, add a file URL and destination, optionally enable reusable-link probing or enter a trusted digest, then start it or run the automatic queue. Added jobs can start automatically while the queue worker is running. The progress bar shows received bytes; details distinguish verified durable checkpoints. Closing the window hides it to the tray; the tray can reopen it, pause the queue or exit the interface. Leave probing disabled for uncertain one-use links.

Use **Import links** for one or more direct file links, automatic filenames and optional folders by extension. Open a file's details to set recurring hours for its named queue. See the [workflow guide](docs/WORKFLOWS.md) for overnight windows, single-use behavior and CLI input formats.

## Build

Prerequisites: Windows 11 x64, Microsoft C++ Build Tools and Windows SDK, and Rust/rustup. The repository pins Rust 1.99.0. Cargo builds libcurl, nghttp2 and SQLite from source; no Python runtime is needed by the application.

```powershell
cargo build --workspace --locked
cargo test --workspace --locked
python -m unittest discover -s tests -v
.\target\debug\idmxl.exe doctor
```

Python 3.11+ is used only for deterministic end-to-end tests. They run a temporary HTTP server on loopback and use generated files. Network-restricted execution sandboxes must permit loopback for the test processes.

For the fault and TLS/HTTP2 fixtures, install `tests/requirements.txt` into a development Python environment and run `scripts/test.ps1 -Faults -Protocols`. The ordinary build excludes fault/test-CA hooks. The optional resource test uses a generated 4,294,967,553-byte file and needs at least 6 GiB free; see TEST-EVIDENCE.md. Test runtimes are not shipped.

## Use the CLI

```powershell
# Queue a reusable file URL and record a trusted digest if one is available.
.\target\debug\idmxl.exe add https://example.org/file.zip --output C:\Downloads\file.zip --replayable

# Run the returned job ID. Ctrl+C pauses it.
.\target\debug\idmxl.exe run <job-id> --connections 2
.\target\debug\idmxl.exe list
.\target\debug\idmxl.exe pause <job-id>
.\target\debug\idmxl.exe run <job-id>

# Run queued jobs automatically, waiting for future scheduled starts.
.\target\debug\idmxl.exe queue-run --watch
.\target\debug\idmxl.exe queue-pause
.\target\debug\idmxl.exe queue-limit 1048576
```

The destination folder must already exist. Existing files are never silently replaced. Job data defaults to `%LOCALAPPDATA%\IDMXL`; use `--state-dir <folder>` for an isolated profile.

Use `add --url-stdin --output <path>` for URLs with secret tokens so those tokens are not placed in process arguments. On Windows, original and resolved URLs are protected with user-scoped DPAPI; the UI/history display removes query strings and fragments. Do not include credentials or tokens in the URL path. Password-bearing URLs are rejected. DPAPI does not protect against malicious code already running as the same user.

Without `--replayable`, the engine skips probing and uses a single request. Interrupted sequential jobs restart safely. With `--replayable`, safe resume requires matching strong identity and a valid range response. Missing/weak validators cause sequential fallback. Refreshing a signed URL is not implemented yet.

Cancel preserves partial files and prevents further running of that job. The current CLI intentionally does not delete downloads or partials. Only one job runs at a time per profile; a second engine receives `engine_busy`.

`policy <job-id>` reads a JSON object from stdin: `queue_name`, `priority` (-1000..1000), `not_before` (UTC Unix milliseconds), optional `deadline`, `connections` (1..4), `retries` (0..5), and `speed_limit` (bytes/s; zero unlimited). Rate changes apply when a job starts. Schedules run only while the logged-in queue worker is alive; missed start times run when next eligible. Explicitly paused jobs stay paused; global queue pause preserves its current job for the next queue start. Failed jobs require an explicit retry/requeue.

`batch` reads a JSON array of up to 1,000 objects with `url`, `destination`, optional `replayable` and `expected_sha256`. Each result is reported separately; identical URL/destination pairs within the input are deduplicated. It queues only. `list` exports redacted history, rather than reusable secret URLs. See [browser handoff setup](docs/BROWSER-HANDOFF.md), [media experiment](docs/MEDIA-BUILD.md) and [release checks](docs/RELEASE-CHECKLIST.md).

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

