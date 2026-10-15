# Feature status

Status on 15 October 2026. "Partial" means a useful subset exists; it does not satisfy the full original feature contract. Full release allocation remains in PLAN.md.

| ID | Capability | Current evidence/status |
|---|---|---|
| F01 | Acceleration | Partial: fixed 1-4 conditional ranges plus experimental CLI adaptive admission; generated per-request-bottleneck benchmark passed. Default/UI integration and wider/shared-bottleneck/HTTP2/disk qualification pending. |
| F02 | Pause/resume/retry | Partial: verified resume, bounded retries, HTTP-date/numeric Retry-After and persistent origin cooldowns; durable attempt counters pending. |
| F03 | Recovery | Partial: process-kill and publication tests; full fault/power-loss qualification pending. |
| F04 | Queues/priorities | Partial: automatic named queues, priorities, checkpoint-based rotation and queue pause/resume; one active job. |
| F05 | Scheduling | Partial: logged-in worker, one-time UTC starts/deadlines and missed-start eligibility; recurrence/wake policies pending. |
| F06 | Limits | Partial: per-job and global queue speed ceilings at admission; strict quotas and metered policies pending. |
| F07 | Categories/folders | Partial: explicit destination and no-overwrite behavior; rules pending. |
| F08 | Browser handoff | Partial: original MV3 explicit-link extension, bounded/idempotent native protocol, public DNS/address policy and setup script; real Edge install/click-through pending. |
| F09 | Browser video button | Pending, Media release. |
| F10 | Video quality | Pending, Media release. |
| F11 | Track combination | G0 feasibility passed for generated original 8K MOV/RGB + WAV/PCM stream copy; product media workflow and streaming-codec matrix pending. |
| F12 | Batches | Partial: bounded JSON file-job import, input deduplication and per-entry results; video batches/playlists pending. |
| F13 | Page links | Pending, Extended. |
| F14 | Grabber | Pending, Extended. |
| F15 | Synchronization | Pending, Extended. |
| F16 | HTTP/HTTPS | Partial: HTTP fixtures plus ALPN HTTP/2 TLS/ranges, untrusted/expired/wrong-host fixtures; broader network/IPv6 qualification pending. |
| F17 | FTP/FTPS | Pending, Extended. |
| F18 | Proxies | Pending; environment proxies disabled. |
| F19 | Authentication | Pending; password-bearing URLs rejected. |
| F20 | Signed-in downloads | Pending; signed public URLs can be protected, but browser sessions are not transferred. |
| F21 | Expired-link refresh | Pending. |
| F22 | Antivirus | Partial: Windows origin-zone metadata; scanner invocation/status pending. |
| F23 | ZIP preview | Pending, Extended. |
| F24 | Clipboard/drag/CLI | Partial: functional JSON CLI and desktop form; clipboard/drag integration pending. |
| F25 | Progress/history | Partial: history/search, received/durable bytes, sampled speed/ETA and actionable cooldowns; large-history/UI performance qualification pending. |
| F26 | Interface | Partial: job/queue/schedule controls, tray, light/dark/system theme and folder reveal; full customization and accessibility qualification pending. |
| F27 | Languages/accessibility | Pending qualification; English baseline. |
| F28 | Installation/updates | Partial: per-user NSIS build, schema backups/idle migration, inventory/notices; signed updates, clean-machine and upgrade/uninstall gates pending. |
| F29 | Completion actions | Partial: explicit reveal-in-folder for completed available files; notifications/exit and advanced actions pending. |
| F30 | Size/integrity | Partial: real 4 GiB-plus transfer/hash through a 397-character Unicode path passed; disk-full, reparse races and filesystem matrix pending. |
| F31 | Sleep/network recovery | Partial: explicit rerun/resume; sleep/network-change qualification pending. |
| F32 | Other app integration | Partial: minimal CLI; supported scoped IPC pending. |
