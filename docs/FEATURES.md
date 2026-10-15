# Feature status

Status on 15 October 2026. "Partial" means a useful subset exists; it does not satisfy the full original feature contract. Full release allocation remains in PLAN.md.

| ID | Capability | Current evidence/status |
|---|---|---|
| F01 | Acceleration | Partial: fixed 1-4 conditional ranges; adaptation and benchmarks pending. |
| F02 | Pause/resume/retry | Partial: public/replayable file jobs; safe sequential restart; basic bounded range retry. |
| F03 | Recovery | Partial: process-kill and publication tests; full fault/power-loss qualification pending. |
| F04 | Queues/priorities | Partial: persistent queued jobs, one active job; automatic scheduling pending. |
| F05 | Scheduling | Pending. |
| F06 | Limits | Partial: per-job speed limit; global/queue limits and quotas pending. |
| F07 | Categories/folders | Partial: explicit destination and no-overwrite behavior; rules pending. |
| F08 | Browser handoff | Pending. |
| F09 | Browser video button | Pending, Media release. |
| F10 | Video quality | Pending, Media release. |
| F11 | Track combination | Pending, Media release. |
| F12 | Batches | Pending; individual jobs only. |
| F13 | Page links | Pending, Extended. |
| F14 | Grabber | Pending, Extended. |
| F15 | Synchronization | Pending, Extended. |
| F16 | HTTP/HTTPS | Partial: HTTP fixtures and TLS defaults; live TLS/HTTP2 qualification pending. |
| F17 | FTP/FTPS | Pending, Extended. |
| F18 | Proxies | Pending; environment proxies disabled. |
| F19 | Authentication | Pending; password-bearing URLs rejected. |
| F20 | Signed-in downloads | Pending; signed public URLs can be protected, but browser sessions are not transferred. |
| F21 | Expired-link refresh | Pending. |
| F22 | Antivirus | Partial: Windows origin-zone metadata; scanner invocation/status pending. |
| F23 | ZIP preview | Pending, Extended. |
| F24 | Clipboard/drag/CLI | Partial: functional JSON CLI; desktop/clipboard integration under development. |
| F25 | Progress/history | Partial: durable progress, redacted persistent history and actionable error codes. |
| F26 | Interface | Desktop shell under development. |
| F27 | Languages/accessibility | Pending qualification; English baseline. |
| F28 | Installation/updates | Pending. |
| F29 | Completion actions | Pending. |
| F30 | Size/integrity | Partial: 64-bit offsets/counters, hashes and disk check; large-file/resource qualification pending. |
| F31 | Sleep/network recovery | Partial: explicit rerun/resume; sleep/network-change qualification pending. |
| F32 | Other app integration | Partial: minimal CLI; supported scoped IPC pending. |
