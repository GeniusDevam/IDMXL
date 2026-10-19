# IDMXL progress

Updated 19 October 2026. This file is the continuation record; checked items describe implemented and verified behavior, not full product parity.

Current checkpoint: extension **0.1.5** and its matching native app add save-location confirmation, four-use folder learning, editable locations, live extension controls, immediate retry, confirmed file deletion and confirmed bulk actions. Native handoff holds Chrome’s filename decision and cancels its original transfer before starting. Browser-assisted downloads remain browser-owned and use Chrome/Edge’s chooser. **182 local checks pass**; native lint, frontend, release/installer build and isolated update qualification pass. [Windows CI for this runtime](https://github.com/GeniusDevam/IDMXL/actions/runs/39292821659) passed. The normal-session update receipt verifies the matching engine/desktop, all fourteen extension files, preserved history IDs and restoration of the previously running queue. The user confirmed actual Chrome 0.1.5 Save/Cancel, live progress and no separate Chrome download. The extension Pause/Resume/Cancel test also passed by user confirmation. See docs/TEST-EVIDENCE.md, docs/DOWNLOAD-CONTROLS.md and docs/evidence/download-controls.json. Earlier milestones are historical.

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
- [x] Durable failed-request accounting and default/UI adaptive integration (Milestone 6); broader acceleration benchmarks remain qualification work.
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

## Milestone 5: browser, media and updater product layers

- [x] Opt-in exact public-folder capture, bounded observation, held native preparation, verified browser cancellation and durable restart/acknowledgement recovery. Unsupported downloads retain browser ownership.
- [x] Clicked media-element source discovery plus desktop media workflow: restricted direct/HLS VOD/static DASH selection, matching audio, audio-only, ordinary queues/limits and retained source tracks.
- [x] Controlled local stream-copy helpers, build-specific hashes, bounded processes/output, source re-verification and recoverable final publication. Original 8K/PCM and AAC HLS/DASH fixtures pass.
- [x] Signed Ed25519 update metadata, freshness/high-water state, hash/size/origin and Windows publisher checks, idle paired snapshots and separate-profile recovery.
- [x] Owner signing/hosting preparation scripts and UPDATES.md; production authority is deliberately unset because the owner has no certificate or endpoint yet.
- [x] Schema v5 with idle v4 backup; prior four completed QA jobs/history remain intact.
- [x] 86 automated cases passed: 29 Rust, 49 Python and 8 extension checks. A separate final-release >4 GiB/397-character Unicode-path rerun also passed, giving 87 distinct cases in this checkpoint. Native lint/build and NSIS packaging passed.
- [x] Isolated package-resource/CLI install and uninstall smoke passed; prior global history remained unchanged. New GUI and clean-machine qualification are separate.
- [x] Runtime/build checkpoint `1885d6a` passed [fresh Windows CI](https://github.com/GeniusDevam/IDMXL/actions/runs/38889150575), including all fixture/frontend and native desktop checks.
- [x] Updated 370-component inventory with 545 notices, 44 review entries and zero CycloneDX schema errors.
- [ ] Real Edge transfer qualification: extension UI is visible, but native-host-not-found prevents acknowledgement. Registry/configuration/stdio checks pass. Browser restart retry was stopped by the UI tool; see TEST-EVIDENCE.md.
- [ ] New media/updater UI click-through, real optional capture permissions, clean-machine installation and trusted production update round trip.
- [ ] Broader media codecs/HDR, named website extractors, complete Core/Extended gates and public distribution audit.

## Continuation instructions

The current code implements the scoped browser/media/updater layers above. Read MEDIA.md, UPDATES.md, BROWSER-HANDOFF.md, FEATURES.md and TEST-EVIDENCE.md before making support claims. The global QA profile is schema v5 with four completed earlier jobs, no successful browser handoff and a valid v4 backup. Preserve it and all download/snapshot files. The host registration points at target/release/idmxl.exe; its QA download folder is .local/browser-qa.

Run scripts/test.ps1 -Faults -Protocols -Media before recovery changes. The controlled helper recipe and matching source/notices ship with the installer; the separate fixture encoder and Python do not. Preview update checks are offline until the owner provides the certificate, metadata key and HTTPS hosting. Do not invent production credentials or mark the trusted-update round trip passed.

Next qualification is real Edge native-host discovery, then explicit/capture/media handoff and native media/update UI. Interface automation stopped because the browser URL could not be verified confidently. The previous tray-menu check was deliberately skipped. Full IDM parity and the remaining feature-matrix gates are not complete.

Interface automation is limited to targeted checks; background builds and tests do not require desktop input. The automation session was closed after testing.

## Milestone 6: everyday download controls

The user selected everyday downloads and media as the first release scope. Extended FTP/crawler/synchronization work remains deferred. The supported user flows and explicit limits are in docs/EVERYDAY-RELEASE.md.

- [x] New reusable desktop/queue jobs default to saved automatic connection tuning, up to four; explicit fixed admission and existing job policies are preserved.
- [x] Failed-request budgets and lifetime transfer-body counters persist across process loss. Explicit failed-job retry resets only the budget, under the transfer lock.
- [x] Review-before-adding paste/drop import, bounded and credential/protocol filtered, with no clipboard polling.
- [x] Renewed links preserve bytes only with an original expected SHA-256, matching origin/path, exact length and strong ranged validator; unproven sources leave checkpoints/source intact.
- [x] Durable unique completion events, optional fixed-text Windows notifications and explicit interface exit that keeps workers running.
- [x] Schema v6 with an idle v5 snapshot; updater snapshot/restore accepts the new schema.

Real Edge connection was retried in its installed extension page: the browser still reports native host not found. The same user owns the browser and registration, paths/configuration exist and direct framed protocol tests pass. No browser security setting was changed. Computer control was released immediately afterward. This is a known unresolved Core browser qualification issue; the product is not represented as full IDM parity or ready for public distribution.


Final local validation: 95 distinct passing checks (32 Rust, 52 Python, 10 Node and a separate real >4 GiB/Unicode-path case), native checks, frontend and NSIS. Final current-user installation preserves the four earlier jobs and files. The official Antigravity IDE 2.5.5 x64 installer was downloaded through the installed IDMXL engine, paused/resumed without first-block refetch, independently hashed and verified as signed by Google LLC; it is left unexecuted in Downloads. There are now five completed jobs and the queue is stopped. Inventory: 379 components / 561 notice files / 44 review entries / zero schema errors. The fresh private CI run is https://github.com/GeniusDevam/IDMXL/actions/runs/38896405636 (status recorded in evidence). New native UI launch timed out; the final Edge diagnostic check awaits user dismissal of its developer-mode security notice. These do not count as completed GUI/browser qualification.


Fresh [Windows CI](https://github.com/GeniusDevam/IDMXL/actions/runs/38896405636) passed for implementation checkpoint `a1ac387`, including all fixture/frontend and native desktop checks. The final evidence follow-up changes documentation only.

## Follow-up: real Edge handoff and installed interface passed

After the user dismissed the developer-mode notice, Edge's connection check reported **IDMXL is connected and ready**. An actual right-click **Queue link in IDMXL** on a controlled loopback page linking to the public FFmpeg HTTPS source archive produced one durable handoff and one queued job. The extension acknowledged the queue operation. The installed engine then completed that job in one sequential body request with no failures. The 12,040,788-byte file has SHA-256 `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`, independently matching the packaged source archive. Its Internet ZoneId is 3. There are now six completed jobs, including all four earlier QA jobs and Antigravity; the queue remains stopped. Neither archive nor Antigravity was executed.

The successful sequence followed registration of the installed engine and a full normal Edge exit/restart. The precise cause of earlier lookup failure was not isolated, and no browser security setting was changed. Earlier pending/host-not-found statements above are historical and superseded for this explicit-link path. Real optional capture and browser-media cases remain unqualified.

The installed IDMXL window launched successfully on this follow-up. Its visible six-job completed history, Antigravity destination/hash and 220/0 request/failure counts, automatic connection setting with maximum four/retries two, import/media dialog layouts and unchecked completion preference were inspected. Dialogs were closed without adding jobs or saving preferences. The explicit **Exit interface — keep downloads running** button exited the interface with an idle queue. This supersedes the earlier launch timeout; actual OS toast presentation, full import/media/paste/drop workflows and an active-transfer Exit-button case are not newly claimed. Earlier close-window background-worker evidence remains separate.

Edge was exited normally to stop temporary diagnostic logging; the task-created loopback server was stopped and computer control released. No runtime code changed, so the existing 95 automated checks and passing CI remain the validation baseline. These additional manual checks are recorded separately in `docs/evidence/edge-native-success.json` (repository root) / `evidence/edge-native-success.json` (this documentation directory).


## Source publication preparation

The README now leads with the implemented file/media features, preview status and a fresh-clone build/launch walkthrough. A separate CLI guide covers protected URL input, queues, policies and expired-link replacement. Browser qualification, unsigned/manual-update status, unsupported workflows and the unselected product license remain explicit. Update documentation distinguishes public source visibility from public binary and updater availability. Machine-specific user-profile prefixes in evidence are normalized without changing results or hashes. No application behavior changed.

## 17 October 2026: automatic capture across websites

Extension 0.1.1 replaces the folder-entry setup with an all-sites enable button and an off switch. It requests optional HTTP/HTTPS and download-observation permissions from that click, captures only files strictly over 1,000,000 bytes, and rechecks configuration, permissions, source, size and browser danger before cancellation. Final redirect/query links must independently pass the engine's exact public range/strong-identity check. Unsupported downloads keep browser ownership. Existing folder settings retain their limits until the new mode is enabled.

Validation: 34 Rust, 53 Python and 28 Node cases passed, with the optional >4 GiB case skipped rather than recounted. A separate release-engine native-protocol test completed the public 12,040,788-byte source archive through a query URL, matched its independent hash and ZoneId=3, and checked held-job blocking, query conflicts and duplicate commit. Browser cancellation in that test was simulated; it does not qualify real Chrome automatic capture. The updated engine and eight installed extension runtime files were verified without opening the user's download profile or changing registration. The desktop interface was left running. New NSIS packaging and inventory schema validation passed; real browser permission/capture checks remain pending. See `docs/evidence/automatic-capture.json`.


Follow-up: the user screenshot confirms Chrome all-sites capture is enabled, with its off switch available. This qualifies activation only; actual automatic takeover remains unverified. Fresh Windows CI for 8b8300d passed: https://github.com/GeniusDevam/IDMXL/actions/runs/39021873201. The follow-up changes evidence only.

Extension 0.1.2 follow-up: the user reported continued browser downloads despite the enabled switch. Added live worker readiness, permission refresh, late-header/early-name and navigation-detachment handling, fragment/length normalization, public-file verification without browser cookies, visible skip reasons and a committed-capture confirmation window. All 46 focused extension checks pass. Native binaries are unchanged. A controlled browser check was inconclusive (`ERR_BLOCKED_BY_CLIENT`); a later user screenshot confirms automatic takeover and the confirmation window for the official Node.js Windows ZIP. A residual Chrome Save As dialog and final file completion/hash remain separate checks. See `docs/evidence/capture-recovery.json`.

Extension 0.1.3/native follow-up: confirmed captures now enable the queue, launch a persistent scheduler and open/restore the desktop app automatically. Added a profile-specific interface lease, launch readiness and durable acknowledgement handling that preserves later manual pauses. Fixed Windows message-pipe inheritance exposed by the new isolated process tests. Public-browser verification of the new open/start flow is pending; the earlier 0.1.2 Chrome capture receipt remains historical.

0.1.3 validation: 141 passing automated checks (36 Rust / 55 Python / 50 Node), native lint, frontend and installer build. A separate release/native desktop test automatically opened IDMXL, completed two official Node.js ZIPs with published hash matches, and restored the same hidden window. Isolated queue stopped and interface exited; actual Chrome open/start remains pending.

0.1.3 Chrome follow-up: updated installed engine/desktop and twelve extension files hash-verified; profile and registration preserved. The user confirmed a normal browser download opened IDMXL and started automatically without Resume. Completion of that specific captured file remains separate from the two independently hashed native desktop test transfers.


19 October final runtime checkpoint: browser-assisted compatibility and the native database writer-conflict fix are saved in runtime commit `42a62c9`. The full local suite passes 168 checks; eight repeated debug and eight release native background cases pass. Windows CI is green. Chrome browser-assisted capture/Pause/Resume is confirmed by the user. Completion/hash of that browser file remains unverified. The normal-Windows update completed: the latest engine, desktop and thirteen extension runtime files were hash-verified, native history IDs were preserved and the previously running queue was restored. The user confirmed their usual history appears. Source/UI scope, unsigned manual updates and broader qualification limits remain as documented above.
