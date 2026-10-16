# IDMXL progress

Updated 16 October 2026. This file is the continuation record; checked items describe implemented and verified behavior, not full product parity.

## Milestone 1: first working transfer engine

- [x] New local Git repository and private `GeniusDevam/IDMXL` GitHub repository.
- [x] Rust workspace, pinned toolchain and Cargo dependency lockfile.
- [x] Per-profile SQLite schema v1, WAL/FULL durability and future-schema rejection.
- [x] Windows DPAPI protection of original/resolved source URLs; redacted history.
- [x] HTTP(S), bounded range probe, manual validated redirects and TLS verification defaults.
- [x] Strong-ETag conditional range transfers on a libcurl multi loop, 1 MiB extents and at most four requests.
- [x] Single-request mode and conservative sequential fallback.
- [x] Positional writes; file flush before extent commit; saved-extent digest verification.
- [x] Pause/cancel commands, Ctrl+C pause and process-kill recovery.
- [x] Trusted SHA-256 checking and no-overwrite publication with persisted file identity.
- [x] Publication recovery after rename but before completion commit.
- [x] Windows Internet-zone metadata before publication.
- [x] Fixed concurrency and per-job speed cap; bounded eligible range retries.
- [x] Nine Rust unit tests and nineteen deterministic HTTP integration tests pass on Windows x64.

## Remaining G0/G1 work

- [x] Initial Tauri desktop build with local bundled frontend, native save dialog and restricted IPC/capabilities.
- [x] Native UI smoke test: add/start a generated file, close after observing it active, reopen, verify completed history and output SHA-256.
- [x] Explicit MV3 handoff extension, bounded/idempotent native host, public-address DNS pinning, setup files and deterministic round-trip tests.
- [ ] Real Edge installation/context-menu click-through and distribution qualification.
- [x] Controlled FFmpeg build and generated original 8K MOV/RGB + WAV/PCM stream-copy experiment; exact configuration/hashes recorded.
- [x] Real ALPN HTTP/2 range fixture; untrusted, expired and wrong-host TLS failures, with revocation checking enabled.
- [x] Test-only abrupt exit at eight write/flush/journal/publication boundaries with recovery checks.
- [ ] Physical storage power-loss and broader fault/filesystem qualification.
- [x] Actual 4,294,967,553-byte transfer, final hash verification and publication through a 397-character Unicode path.
- [ ] Disk-full, permissions, reparse races and removable/network filesystem qualification.
- [x] Numeric/HTTP-date Retry-After and persistent origin cooldowns across restarts and other queued jobs.
- [x] Opt-in adaptive admission controller, bounded 1..4, multi-sample decisions/backoff and controlled benchmark with verified bytes.
- [ ] Durable retry-attempt accounting, default/UI adaptive integration and broad acceleration benchmarks.
- [x] Automatic named queues, priority, one-time schedules/deadlines, checkpoint-based rotation and per-job/global queue speed caps.
- [x] Recurring weekday/overnight queue admission hours in computer-local time, with persisted settings and safe handling of active transfers.
- [ ] Wake scheduling, full DST/timezone-change qualification, strict quotas, dynamic/global multi-job budgets and metered-network policy.

## Milestone 2: initial native desktop preview

Implemented an original desktop UI with persistent job list, search, status summaries, add-file form, optional expected hash, explicit replay choice, start/pause/cancel and job details. A detached CLI process owns transfers; the UI does not own network bytes. The standalone debug build embeds the frontend and does not need the Vite server.

The native smoke test created only a generated ~3 MiB file. Output matched the fixture SHA-256. The app was reopened with its completed history intact. Windows UI Automation exposed labels, buttons, inputs and progress indicators; full keyboard/screen-reader, resize and accessibility qualification remain pending. The app has been left open for review.

Build and test helpers, the adopted source PDF/plan, architecture decisions, dependency notes, test evidence and all F01-F32 statuses are saved in the repo. Generated toolchains, builds, caches, databases and test downloads are excluded from commits.

## Milestone 3: queue, browser protocol and release foundations

- [x] Live received/durable byte counters, sampled speed and ETA, including sequential bodies before checkpoint.
- [x] Desktop queue controls, schedule editor, themes, system tray, Ctrl+N and explicit reveal-in-folder.
- [x] Bounded JSON file-job batch import with input deduplication and per-entry results.
- [x] Schema v3; idle-engine upgrade guard and pre-migration v1/v2 snapshots. Future versions are rejected.
- [x] Sixteen Rust tests, thirty-six ordinary Python fixture tests and two extension tests pass. The separate real large-file case also passes (55 top-level cases in total).
- [x] TypeScript production build and native/engine formatting and Clippy checks pass.
- [x] Per-user NSIS installer built. CLI VCRuntime dependency fixed; final PE imports contain no external VCRUNTIME140 DLL requirement.
- [x] Installer smoke on this development machine: isolated installation, expected resources and installed-engine doctor pass.
- [x] Updated installed-window click-through: native save dialog, trusted hash/reusable-link input, queue policy save, queue pause/resume, explicit job pause/requeue and close-to-tray background completion. Generated bytes/history/hash/Internet-zone metadata verified.
- [x] Isolated development-machine uninstall preservation smoke: installer files, shortcuts and uninstall entry removed; history, saved queue policy and both generated downloads retained.
- [ ] Tray-menu Show/Pause/Exit click-through (user chose to continue without it), full accessibility and clean-machine/upgrade qualification.
- [x] Generated CycloneDX inventory: 353 components, 516 collected notice files and 44 missing-root-notice review entries. Full distribution audit remains open.
- [x] Windows CI workflow, reproducible test/build/setup helpers, media experiment and release/recovery documentation saved.
- [x] Corrected fresh-checkout Windows CI passed for code checkpoint `b26352b`, including native desktop checks.

The media experiment proves only the recorded generated MOV/RGB + WAV/PCM path. It does not implement the Media release, qualify streaming codecs/HDR or establish extractor/site support. The helper and Python are not bundled in the Core installer. The installer is unsigned; no update endpoint or production signing key is configured.

The opt-in adaptive CLI controller starts at one admission, averages two one-second samples, probes one extra slot only within the configured maximum, rejects non-improving trials, backs off on sustained regressions and holds at one after throttling/retry trouble. Capped/default/desktop queues retain fixed admission. A generated 16 MiB localhost fixture with an artificial 512 KiB/s per-request bottleneck completed in 34.299 s fixed-one versus 14.331 s adaptive-max-four, peaking at four outstanding body requests. This is one declared scenario, not a browser comparison or general acceleration guarantee. Exact machine/limits/results are in `docs/evidence/adaptive-benchmark.json`.

## Milestone 4: everyday download workflows

- [x] Desktop Import links: up to 1,000 direct file links, within-list deduplication, per-entry results and correction of rejected lines without resubmitting accepted ones.
- [x] Saved folder preferences and editable extension rules; categorized subfolders, sanitized filenames and numbered collision handling. Existing files remain intact.
- [x] Atomic filename reservation against queued history, including concurrent imports. Explicit destinations also reject existing reservations.
- [x] Recurring local weekday/overnight queue hours, persisted per named queue. Blocked queues do not prevent other eligible queues from running.
- [x] A window ending stops segmented transfers after a new durable checkpoint; single-stream/one-use transfers finish their current request. Individual Start/Resume runs immediately.
- [x] Schema v4, pre-v3-upgrade backup, and migration guard requiring both engine and queue-worker locks to be idle.
- [x] Native import smoke: two generated files categorized into Documents/Archives, one duplicate skipped, recurring settings reopened, jobs stayed untouched outside hours and completed after the restriction was removed. Independent hashes and ZoneId=3 match; prior generated files remain unchanged.
- [x] Nineteen Rust checks, forty-two ordinary Python methods and two extension checks passed (63 this run). The earlier separate 4 GiB-plus case remains recorded, not rerun here.
- [x] Native Clippy, TypeScript/Vite and release/NSIS build passed. Bundle builds now refresh the checkout launcher's engine companion as well as the packaged sidecar.
- [x] Refreshed 353-component inventory, 516 collected notices and 44 review entries; official CycloneDX 1.6 schema validation reports zero errors.

The native folder picker opened and cancelled, but its editable child control was unavailable in the automation cache. Entering the folder directly in the app completed the functional import test. Full picker-selection, accessibility, clean-machine and timezone-transition matrices remain separate checks. The temporary folder preference was restored after QA; generated history/files were retained. See `docs/WORKFLOWS.md` and `docs/evidence/workflows.json`.

## Later release gates

G2: remaining desktop workflows, default/UI adaptive integration, durable retry accounting, keyboard/screen-reader checks and real-browser qualification. Recurring queue hours and import folder rules now have the scoped evidence above.

G3: per-user installer, clean-machine tests, signed updates, migration/rollback, SBOM/notices and performance evidence.

G4: direct media, restricted clear HLS/DASH VOD, exact source selection, controlled stream-copy helper, 8K/audio/HDR qualification and named supported extractors.

G5: add selected extended workflows from the full plan, with independent tests and support statements.

## Continuation instructions

The current release preview includes recurring queue hours, desktop link import and editable folder rules, with 63 passing automated checks plus the scoped native evidence in Milestone 4. The global QA profile is now schema v4 with four completed generated files; its pre-upgrade v3 snapshot preserves the two earlier jobs. Keep that profile, its snapshots and all generated files intact. QA folder preferences were restored and its queue was stopped. The isolated `.local/installation-smoke` installation had already been uninstalled with preservation verified in the previous milestone.

The source launcher prefers the release checkout build, now paired with the freshly built engine even after a bundle build. The new release was checked in the native UI; final displayed paths omit the Windows extended-path prefix. Exported unsigned installer, inventory, notices, workflow guide and evidence are in `outputs/IDMXL-Preview`. The prior CSS checkpoint's CI run `38825092235` was subsequently verified successful. A later workflow-code CI run must be checked separately. Tray-menu click-through remains skipped at the user's request; the folder picker's child-control cache limitation is recorded in TEST-EVIDENCE.md.

Read `docs/PLAN.md`, `docs/ARCHITECTURE.md`, `docs/WORKFLOWS.md`, `docs/TEST-EVIDENCE.md` and `docs/RELEASE-CHECKLIST.md`. Run `scripts/test.ps1 -Faults -Protocols` before changing recovery behavior. Keep secret URLs on stdin and preserve redacted JSON. Next priorities are real Edge qualification, default/UI adaptive integration, durable retry accounting, clean-machine upgrade/update recovery and remaining Core workflows, followed by the scoped Media release. Do not claim Core/Media/Extended complete while their unchecked work remains.

