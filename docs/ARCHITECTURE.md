# Initial architecture decisions

These ADRs retain milestone history. Later decisions supersede earlier scope statements; ADR-011 records the current recurring-hours and import behavior.

## ADR-001: keep the planned engine stack

Rust owns state, transport and storage. libcurl is statically built with Schannel and nghttp2 on Windows; SQLite is bundled. The `doctor` command reports linked versions. Real ALPN HTTP/2 range transfers passed a TLS fixture, alongside untrusted/expired/hostname-mismatch failures. Test trust uses a temporary CA and signed CRL; Windows trust and production TLS verification remain unchanged.

## ADR-002: make replay an explicit user choice

Unknown URLs default to a single sequential request. `--replayable` permits one bounded range probe and subsequent requests. Ranged mode requires a strong quoted ETag, known total and exact Content-Range. Every extent request uses If-Match. A changed/ignored/malformed response fails before its body is written. A subsequent run probes again; incompatible identity creates a new generation and discards old extents.

Redirects are followed explicitly with the URL parser, restricted to HTTP(S) and without HTTPS-to-HTTP downgrade. Environment proxies are disabled. No cookie or user-header forwarding exists. Explicit desktop/CLI URLs can address an intranet; browser handoffs persist a public-only policy. Each browser-origin request validates all resolved addresses and pins that request's DNS answers, including after redirects. See BROWSER-HANDOFF.md.

## ADR-003: durable extents and publication intent

Each fixed 1 MiB extent has a generation, offset, length and SHA-256. Network callbacks write positionally to a unique staging file beside the final destination. Completed extents are flushed before SQLite records them. An exclusive profile lock prevents two transfer engines; short CLI control transactions can still enqueue or request pause while the engine runs.

Resume verifies the local hashes of recorded extents. Partial in-flight extents are fetched again. Sequential jobs safely restart after interruption. New extents are scheduled lazily with a bounded retry queue; no file-sized buffer or full pending-file index is used. Recovery currently loads saved-extent metadata and must receive a compact/paged representation before very-large-file qualification.

Final verification records a publication intent with expected size, computed hash and Windows file identity. Zone.Identifier is written before a no-replacement MoveFileExW operation. Completion is committed afterward. Recovery requires matching file identity and hash, so an unrelated identical destination is not mistaken for the renamed staging file. Public scanner execution is not implemented.

## ADR-004: actual scope of the first milestone

The original milestone used one manually run file job and fixed range concurrency. Subsequent work adds automatic scheduling, browser handoff, live progress, migrations and installer generation; see ADR-007 onward. Default/UI adaptive tuning, per-extent retry-attempt persistence, full Media/Extended functionality and signed updates remain pending. An experimental opt-in adaptive CLI controller has a bounded fixture/benchmark. Zero software licence fees do not establish binary distribution compliance; see DEPENDENCIES.md.

## ADR-005: local secrets and source provenance

Windows source URLs are encrypted using per-user DPAPI. Query strings/fragments are omitted from display/history. Token-bearing paths are not supported as a secret-storage workflow. Use stdin rather than process arguments for signed URLs. The adopted PDF and Markdown plan are retained in docs, with a recorded SHA-256, and do not override newer direct user instructions.

Source PDF SHA-256: `FDCF0AB97B20C578217009CF744133E0D0797EBCE2F20328B2B79875144C0B85`.

## ADR-006: desktop lifecycle

The Tauri window invokes narrow add/list/control/start commands against the same store and launches a fixed CLI executable for transfer work. URLs do not enter the CLI process arguments. Windows CREATE_NO_WINDOW hides the helper console; dropping/closing the parent window does not intentionally terminate the engine process. A reaper thread closes process handles while the window is alive. History polling reconciles orphaned active states only after acquiring the profile lock.

The frontend has local assets, no remote executable pages, a restrictive CSP and only native save-dialog permission beyond default core capabilities. Debug builds use the checkout engine; release builds use the fixed executable beside the app. A system tray menu can reopen the hidden window, pause queue admission or exit the interface while transfers continue. The UI shows received and durable bytes separately, with bounded progress updates, speed and ETA. Signing and clean-machine qualification remain release work.

## ADR-007: conservative automatic queues

One process owns a scheduler lock and admits one active job at a time through the existing engine lock. Persisted policies provide queue name, priority, UTC start/deadline, connection/retry bounds and rate ceiling. Global and per-job ceilings use their minimum at admission. The scheduler rotates named queues by last service, then priority/FIFO within a queue. A replayable ranged job can yield after 30 seconds and a newly durable checkpoint. Sequential jobs are not interrupted merely to rotate queues. Explicit per-job pause persists; queue pause requeues its current safely paused job for the next queue start.

Schedules require the logged-in worker to remain running. Missed start times become eligible; expired deadlines block admission. There is no wake-from-sleep, Task Scheduler service, recurring plan or strict transfer quota. Numeric/HTTP-date Retry-After deadlines persist per origin, survive process restarts and block early admission by other jobs. Short eligible range retries still use a bounded in-memory work queue; retry-attempt counts are not yet durable.

## ADR-008: received progress is not a durable claim

Receivers count accepted bytes separately from extent commits. At approximately 250 ms intervals, the engine stores received bytes, sampled speed, known total and ETA. Sequential bodies can report progress before the whole request is checkpointed. The interface polls at 750 ms. All byte counters crossing IPC use decimal strings. Recovery resets volatile progress to the durable count; only flushed and journaled extents are reusable.

## ADR-009: migration and distribution boundaries

Schema v3 adds queue policies, runtime counters, durable handoff IDs, origin deadlines and service ordering. An idle engine lock is required before upgrading an existing profile. VACUUM INTO produces a version-labelled snapshot before the older schema changes; origin backfill and schema-v3 changes share a transaction. Future schema versions are rejected. Backups are local, retain DPAPI-protected URLs, and do not constitute a cross-machine portable export.

The per-user NSIS installer bundles the desktop, engine, original extension and setup instructions. Both executables use static VCRuntime linkage with Windows' UCRT; PE import inspection checks the extra VCRUNTIME DLL is absent. Updater/signing, clean-machine, upgrade/uninstall and licence review gates are distinct. The resolved CycloneDX inventory includes build/test packages and flags missing notice texts; it is not a completed distribution audit.

## ADR-010: qualify adaptive admission separately

`run --adaptive` is experimental and opt-in, restricted to uncapped conditional range jobs. It starts at one and uses pairs of one-second goodput samples, one-slot trials, a minimum trial gain, conservative cooldowns and sustained-regression reduction. Active leases drain normally; identity validation, positional writes and journal boundaries are unchanged. Retry/throttle trouble reduces admission to one, and Retry-After still governs actual request admission. The configured maximum is never exceeded. One-use/sequential sources are not probed or replayed by this flag. Rate-capped jobs and desktop queues keep their established fixed behavior until dynamic rate allocation is separately qualified.

## ADR-011: recurring admission and generated destinations

Schema v4 persists named-queue windows in local civil time. Chrono converts the current Windows time into weekday/minute values; pure calendar logic handles same-day, overnight and all-day windows. Admission scans the existing queue ordering until it finds an eligible queue, so a closed queue cannot starve another one. Publication recovery can proceed without waiting for recurring hours. Full DST/system-time-change and wake qualification remain separate work.

The watcher yields ranged work only after a newly durable extent when its window closes. Sequential/one-use work finishes its original request; it is not automatically interrupted and replayed just to enforce recurring hours. Completed jobs do not generate new repeated downloads. Explicit per-job Start/Resume bypasses automatic scheduling and now forwards its saved connection/retry/rate settings to the CLI.

Import links uses the engine's URL validator, DPAPI storage and no-overwrite publisher. It decodes/sanitizes URL path filenames, applies validated extension-to-single-subfolder rules, and reserves a free numbered destination inside the job's immediate SQLite transaction. ASCII case-insensitive reservation lookup covers ordinary Windows filename collisions; filesystem publication remains the final no-overwrite authority. Existing category reparse points are refused. Broader Unicode case-alias and reparse-race matrices are not claimed.

The whole input and each queue/rule are bounded. Per-entry results omit secret source tokens. Accepted input lines are removed in the desktop before a correction can resubmit them. A repeat desktop/CLI import remains an explicit new import rather than the durable idempotent browser-host protocol. Folder preferences contain no source URLs. Migrations require both the engine and scheduler to be idle; a v3 snapshot is retained before v4 changes.


## ADR-012: held browser ownership transfer

Schema v5 records browser capture phases and expiring preparation. A held job is excluded from every ordinary run path. Preparation independently verifies public DNS and exact strong representation; commit requires confirmed browser cancellation. The extension journals redacted intent before each ownership transition and reconciles durable IDs after worker restart. Unsupported or ambiguous cases preserve browser ownership or request attention. Explicit links and selected media use the same bounded host with separate protocol versions.

## ADR-013: media belongs to the existing engine

A media parent stores a protected frozen plan and hidden component jobs. One engine lock spans download, verified local assembly and ordinary recoverable publication. Pause/queue limits use existing admission; byte-range sources are downloaded whole and sliced locally. Helpers accept no network or arbitrary options, have build-specific hashes and Windows process/resource limits, and stream-copy into a verified output. This is not a full Windows sandbox. Source tracks remain available on failure; wider codec/HDR and extractor qualification is independent.

## ADR-014: release authority and paired recovery

Preview builds have no update authority or network check. Production builds embed one HTTPS endpoint/artifact origin, Ed25519 metadata key and Authenticode publisher identity. Signed metadata binds product/channel/target/version, increasing sequence, freshness, hash and size; persisted high-water state rejects replay/equivocation. Installation validates publisher identity and takes an idle, integrity-checked database snapshot with matching old binaries. Recovery restores a separate profile from a protected receipt, preserving current history and downloads. Operator credentials/hosting and a real trusted upgrade rehearsal are external release gates.
