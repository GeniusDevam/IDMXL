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

- [ ] Initial Tauri desktop build and engine-process lifecycle verification.
- [ ] Edge native-host installation/round-trip feasibility and explicit handoff protocol.
- [ ] Direct audio/video remux build experiment and dependency configuration record.
- [ ] HTTP/2 fixture negotiation and TLS/certificate-failure fixtures (linked capability alone is not qualification).
- [ ] Full fault injection at every write/flush/journal/publication boundary; real storage power-loss qualification.
- [ ] Disk-full, permissions, long paths, >4 GiB, reparse races and filesystem support qualification.
- [ ] Persistent advanced retry scheduling; HTTP-date Retry-After; adaptive concurrency and benchmarking.
- [ ] Multiple queued-job scheduling, global/queue budgets and strict quotas.

## Later release gates

G2: complete desktop workflows, queues, schedules, keyboard/screen-reader checks and qualified explicit browser handoff.

G3: per-user installer, clean-machine tests, signed updates, migration/rollback, SBOM/notices and performance evidence.

G4: direct media, restricted clear HLS/DASH VOD, exact source selection, controlled stream-copy helper, 8K/audio/HDR qualification and named supported extractors.

G5: add selected extended workflows from the full plan, with independent tests and support statements.

## Continuation instructions

Read `docs/PLAN.md`, `docs/ARCHITECTURE.md` and `docs/TEST-EVIDENCE.md`. Run the current Rust and HTTP suites before changing recovery behavior. Preserve the CLI's redacted JSON contract and keep token-bearing URLs on stdin. Add regression cases for any new failure mode. Do not claim G0/G1 or the Core release complete while the unchecked qualification work remains.

