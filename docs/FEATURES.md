# Feature status

Status on 16 October 2026. "Partial" means a useful subset exists; it does not satisfy the full original feature contract. Full release allocation remains in PLAN.md.

| ID | Capability | Current evidence/status |
|---|---|---|
| F01 | Acceleration | Implemented for the scoped release: 1-4 conditional ranges, saved automatic/fixed controls and automatic tuning for new reusable desktop/queue jobs. Generated bottleneck/hash tests pass; wider performance qualification remains. |
| F02 | Pause/resume/retry | Implemented for the scoped release: verified resume, persisted failed-request budgets/counts across process loss, bounded retries, HTTP-date/numeric Retry-After and persistent origin cooldowns. Explicit retry of a failed job grants a fresh budget. |
| F03 | Recovery | Partial: process-kill and publication tests; full fault/power-loss qualification pending. |
| F04 | Queues/priorities | Partial: automatic named queues, priorities, checkpoint-based rotation and queue pause/resume; one active job. |
| F05 | Scheduling | Partial: one-time UTC starts/deadlines plus persisted weekday/overnight local queue windows, safe range checkpointing and single-stream finish behavior; wake and full DST/timezone-change qualification pending. |
| F06 | Limits | Partial: per-job and global queue speed ceilings at admission; strict quotas and metered policies pending. |
| F07 | Categories/folders | Partial: editable extension-to-subfolder rules in desktop link import, safe generated names, collision reservation and no-overwrite publication; rules do not move existing files or override explicit destinations. |
| F08 | Browser handoff | Partial: explicit links plus opt-in public-folder capture with held preparation, verified cancellation and restart recovery; bounded native protocol and public DNS policy. Automated checks pass; real Edge evidence is in TEST-EVIDENCE.md. |
| F09 | Browser video button | Partial: clicked popup discovers video/audio element sources and queues supported public media. Blob/session-bound players and named site extractors are unsupported. |
| F10 | Video quality | Partial: exact rendition/audio choices for direct files, restricted clear HLS VOD and static DASH. No conversion or silent quality fallback; broader codecs/HDR qualification pending. |
| F11 | Track combination | Partial: product queue, controlled helpers, retained tracks, audio-only and stream-copy verification. Original 8K MOV/RGB + PCM, HLS AAC ranges and DASH AAC/fMP4 fixtures pass; broader codec matrix pending. |
| F12 | Batches | Partial: desktop plain-link import and bounded JSON file-job/link batches, deduplication, category folders and per-entry results; page discovery/video batches/playlists pending. |
| F13 | Page links | Pending, Extended. |
| F14 | Grabber | Pending, Extended. |
| F15 | Synchronization | Pending, Extended. |
| F16 | HTTP/HTTPS | Partial: HTTP fixtures plus ALPN HTTP/2 TLS/ranges, untrusted/expired/wrong-host fixtures; broader network/IPv6 qualification pending. |
| F17 | FTP/FTPS | Pending, Extended. |
| F18 | Proxies | Pending; environment proxies disabled. |
| F19 | Authentication | Pending; password-bearing URLs rejected. |
| F20 | Signed-in downloads | Pending; signed public URLs can be protected, but browser sessions are not transferred. |
| F21 | Expired-link refresh | Scoped implementation: replacement for idle ordinary jobs. Keeping saved bytes requires an original expected SHA-256 plus matching origin/path, strong ETag, length and range support; rejected candidates preserve the old source. Media/capture renewal is unsupported. |
| F22 | Antivirus | Partial: Windows origin-zone metadata; scanner invocation/status pending. |
| F23 | ZIP preview | Pending, Extended. |
| F24 | Clipboard/drag/CLI | Implemented for the scoped release: JSON CLI, desktop form and review-before-adding Ctrl+V/text-link drop import. Clipboard polling and automatic enqueue are absent. |
| F25 | Progress/history | Partial: history/search, received/durable bytes, sampled speed/ETA and actionable cooldowns; large-history/UI performance qualification pending. |
| F26 | Interface | Partial: job/queue/schedule controls, tray, light/dark/system theme and folder reveal; full customization and accessibility qualification pending. |
| F27 | Languages/accessibility | Pending qualification; English baseline. |
| F28 | Installation/updates | Partial: NSIS, schema-v6 idle migration/backups, Ed25519 metadata and publisher-checked updater, paired recovery and owner setup tools. Production authority is unset; trusted update round trip, clean-machine and distribution gates remain. |
| F29 | Completion actions | Scoped implementation: explicit reveal-in-folder, optional fixed-text completion notifications while the interface runs, and explicit interface exit with background downloads retained. Windows suppression/installed notification qualification and advanced actions remain. |
| F30 | Size/integrity | Partial: real 4 GiB-plus transfer/hash through a 397-character Unicode path passed; disk-full, reparse races and filesystem matrix pending. |
| F31 | Sleep/network recovery | Partial: explicit rerun/resume; sleep/network-change qualification pending. |
| F32 | Other app integration | Partial: minimal CLI; supported scoped IPC pending. |
