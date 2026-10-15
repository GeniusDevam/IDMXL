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
- [ ] Recurring schedules, strict quotas, dynamic/global multi-job budgets and metered-network policy.

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

## Later release gates

G2: recurring schedules, category rules, remaining desktop workflows, keyboard/screen-reader checks and real-browser qualification.

G3: per-user installer, clean-machine tests, signed updates, migration/rollback, SBOM/notices and performance evidence.

G4: direct media, restricted clear HLS/DASH VOD, exact source selection, controlled stream-copy helper, 8K/audio/HDR qualification and named supported extractors.

G5: add selected extended workflows from the full plan, with independent tests and support statements.

## Continuation instructions

The 16 October retry recovered the minimized installed window successfully. Native pause/requeue and background completion passed. The isolated `.local/installation-smoke` installation was then uninstalled successfully, after backing up its QA history and stopping its test processes; both generated downloads and the global profile remain intact. The tray menu is not exposed as a target by the UI automation helper, and the user chose to continue without that click-through. Dark-mode contrast defects found during visual QA were corrected. The source launcher prefers the release checkout build. Exported unsigned installer, inventory, notice archive and build manifest are in the sibling `outputs/IDMXL-Preview` folder.

The rebuilt release was visually checked, retained both completed jobs and passed search, Ctrl+N/Escape and folder-reveal checks. System theme was restored. The final installer rebuilt successfully after stopping the running QA executable, and its exported checksum matches. The latest CSS checkpoint's fresh CI run `38825092235` was queued when saved; do not label that run green without checking it. The earlier code checkpoint's green CI and 55 automated cases remain recorded separately.

Read `docs/PLAN.md`, `docs/ARCHITECTURE.md`, `docs/TEST-EVIDENCE.md` and `docs/RELEASE-CHECKLIST.md`. Run `scripts/test.ps1 -Faults -Protocols` before changing recovery behavior. Keep secret URLs on stdin and preserve redacted JSON. Next priorities are completing the installed/native and real Edge checks, clean-machine install/upgrade/uninstall, signed update recovery, adaptive tuning and the remaining Core workflows. Do not claim the Core/Media/Extended releases complete while their unchecked work remains.

