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
- [ ] Edge native-host installation/round-trip feasibility and explicit handoff protocol.
- [ ] Direct audio/video remux build experiment and dependency configuration record.
- [ ] HTTP/2 fixture negotiation and TLS/certificate-failure fixtures (linked capability alone is not qualification).
- [ ] Full fault injection at every write/flush/journal/publication boundary; real storage power-loss qualification.
- [ ] Disk-full, permissions, long paths, >4 GiB, reparse races and filesystem support qualification.
- [ ] Persistent advanced retry scheduling; HTTP-date Retry-After; adaptive concurrency and benchmarking.
- [ ] Multiple queued-job scheduling, global/queue budgets and strict quotas.

## Milestone 2: initial native desktop preview

Implemented an original desktop UI with persistent job list, search, status summaries, add-file form, optional expected hash, explicit replay choice, start/pause/cancel and job details. A detached CLI process owns transfers; the UI does not own network bytes. The standalone debug build embeds the frontend and does not need the Vite server.

The native smoke test created only a generated ~3 MiB file. Output matched the fixture SHA-256. The app was reopened with its completed history intact. Windows UI Automation exposed labels, buttons, inputs and progress indicators; full keyboard/screen-reader, resize and accessibility qualification remain pending. The app has been left open for review.

Build and test helpers, the adopted source PDF/plan, architecture decisions, dependency notes, test evidence and all F01-F32 statuses are saved in the repo. Generated toolchains, builds, caches, databases and test downloads are excluded from commits.

## Later release gates

G2: complete desktop workflows, queues, schedules, keyboard/screen-reader checks and qualified explicit browser handoff.

G3: per-user installer, clean-machine tests, signed updates, migration/rollback, SBOM/notices and performance evidence.

G4: direct media, restricted clear HLS/DASH VOD, exact source selection, controlled stream-copy helper, 8K/audio/HDR qualification and named supported extractors.

G5: add selected extended workflows from the full plan, with independent tests and support statements.

## Continuation instructions

Read `docs/PLAN.md`, `docs/ARCHITECTURE.md` and `docs/TEST-EVIDENCE.md`. Run `scripts/test.ps1` before changing recovery behavior. Preserve the CLI's redacted JSON contract and keep token-bearing URLs on stdin. Add regression cases for any new failure mode. Next priorities are browser/native-host feasibility, proper in-flight progress, TLS/HTTP2 and disk/path fixtures, and the remux experiment. Do not claim G0/G1 or the Core release complete while unchecked qualification work remains.

