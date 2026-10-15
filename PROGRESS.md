# IDMXL progress

Updated 15 October 2026. This file is the continuation record; checked items describe implemented and verified behavior, not full product parity.

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
- [ ] Durable retry-attempt accounting, adaptive concurrency and acceleration benchmarks.
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
- [x] Thirteen Rust tests, thirty-three ordinary Python fixture tests and two extension tests pass. The separate real large-file case also passes (49 top-level cases in total).
- [x] TypeScript production build and native/engine formatting and Clippy checks pass.
- [x] Per-user NSIS installer built. CLI VCRuntime dependency fixed; final PE imports contain no external VCRUNTIME140 DLL requirement.
- [x] Installer smoke on this development machine: isolated installation, expected resources and installed-engine doctor pass.
- [ ] Updated installed-window click-through/visual test: accessibility tree exposes queue/theme controls, but the Windows helper failed to activate the captured window twice. Awaiting a restored window.
- [ ] Uninstall preservation smoke and clean-machine/upgrade qualification.
- [x] Generated CycloneDX inventory: 353 components, 516 collected notice files and 44 missing-root-notice review entries. Full distribution audit remains open.
- [x] Windows CI workflow, reproducible test/build/setup helpers, media experiment and release/recovery documentation saved.

The media experiment proves only the recorded generated MOV/RGB + WAV/PCM path. It does not implement the Media release, qualify streaming codecs/HDR or establish extractor/site support. The helper and Python are not bundled in the Core installer. The installer is unsigned; no update endpoint or production signing key is configured.

## Later release gates

G2: recurring schedules, category rules, remaining desktop workflows, keyboard/screen-reader checks and real-browser qualification.

G3: per-user installer, clean-machine tests, signed updates, migration/rollback, SBOM/notices and performance evidence.

G4: direct media, restricted clear HLS/DASH VOD, exact source selection, controlled stream-copy helper, 8K/audio/HDR qualification and named supported extractors.

G5: add selected extended workflows from the full plan, with independent tests and support statements.

## Continuation instructions

Read `docs/PLAN.md`, `docs/ARCHITECTURE.md`, `docs/TEST-EVIDENCE.md` and `docs/RELEASE-CHECKLIST.md`. Run `scripts/test.ps1 -Faults -Protocols` before changing recovery behavior. Keep secret URLs on stdin and preserve redacted JSON. Next priorities are completing the installed/native and real Edge checks, clean-machine install/upgrade/uninstall, signed update recovery, adaptive tuning and the remaining Core workflows. Do not claim the Core/Media/Extended releases complete while their unchecked work remains.

