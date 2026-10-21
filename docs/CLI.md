# Command-line usage

Build the engine with `cargo build --workspace --locked` from the repository root. This creates `target/debug/idmxl.exe`; a release desktop build also creates `target/release/idmxl.exe`. Run `idmxl --help` or a subcommand's `--help` for its complete options. A built application does not need Python.

## Queue, run and pause

Create the destination folder first. Existing files are never silently replaced.

```powershell
# Queue a reusable public file. Omit --replayable for unknown/one-use links.
.\target\debug\idmxl.exe add https://example.org/file.zip --output C:\Downloads\file.zip --replayable

# Replace <job-id> with the ID returned by add. Ctrl+C pauses a running job.
.\target\debug\idmxl.exe run <job-id> --connections 2
.\target\debug\idmxl.exe list
.\target\debug\idmxl.exe pause <job-id>
.\target\debug\idmxl.exe run <job-id>

# Process queued jobs and wait for future eligible starts.
.\target\debug\idmxl.exe queue-run --watch
.\target\debug\idmxl.exe queue-pause
.\target\debug\idmxl.exe queue-limit 1048576
```

Only one transfer runs at a time per profile; a second engine receives `engine_busy`. Cancelling a job preserves partial data and prevents that job from running again. The CLI does not delete download or partial files.

State defaults to `%LOCALAPPDATA%\IDMXL`. Use the global `--state-dir <folder>` option for an isolated profile. `doctor` reports the linked engine capabilities.

## Source URLs and integrity

Use `add --url-stdin --output <path>` for URLs containing secret tokens so they are not placed in process arguments. On Windows, original and resolved source URLs are protected with user-scoped DPAPI. History/UI remove query strings and fragments. Do not put credentials in the URL path; password-bearing URLs are rejected. This does not protect against malicious code running as the same Windows user.

Pass `--sha256 <trusted-digest>` to `add` when a trusted expected hash is available. Without `--replayable`, IDMXL skips probing and uses one body request; interrupted jobs restart safely. Reusable links require strong identity and valid ranges before saved bytes can be resumed. Missing or weak validators cause sequential fallback.

## Saved policies

`policy <job-id>` accepts a JSON object on stdin with `queue_name`, `priority` (-1000..1000), `not_before` (UTC Unix milliseconds), optional `deadline`, `connections` (1..8), `retries` (0..5), `adaptive` (boolean) and `speed_limit` (bytes/second; zero unlimited).

Rate changes apply when a job starts. Schedules run while the logged-in queue worker is alive; missed starts run when next eligible. Explicitly paused jobs stay paused. A queue pause preserves its current job for the next queue start. Failed jobs require explicit retry/requeue; that grants a new failed-request budget while retaining historical request counts.

`batch` accepts a JSON array of up to 1,000 objects with `url`, `destination`, optional `replayable` and `expected_sha256`. It queues only, reports each result and deduplicates identical URL/destination pairs within the input. `list` exports redacted history, not reusable secret URLs. See [WORKFLOWS.md](WORKFLOWS.md) for link imports, folder rules and recurring windows.

## Replace an expired file link

`refresh-source <job-id>` accepts a JSON object with a `url` field on stdin. The ordinary job must be paused, failed or queued and the engine must be idle. Browser capture and media jobs require a new job.

With no saved bytes, the replacement can be used on the next run. Keeping saved bytes requires the original expected SHA-256, the same origin/path, no HTTPS downgrade, a matching strong ETag, exact length and a valid range probe. Rejection leaves the old source and checkpoints intact. See [EVERYDAY-RELEASE.md](EVERYDAY-RELEASE.md).
