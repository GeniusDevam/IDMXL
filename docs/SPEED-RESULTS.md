# IDMXL speed update and large-file comparison

IDMXL completed faster than IDM in this large-file pair. IDMXL used **15.91% less time** than IDM. This is one source and one same-session pair, not proof that IDMXL is as fast on every website or connection.

## Fresh public comparison

Both apps downloaded the same official Blender Tears of Steel DCP ZIP: **14,004,054,153 bytes**, using the same Windows PC and a configured four-connection ceiling. The main comparison ran IDM first and IDMXL second on 23 October 2026. No other test downloads, builds or large-file validation ran concurrently with either timed transfer.

| App | Completion time | Average payload rate | Observed transfer mode |
| --- | --- | --- | --- |
| IDM (idm-r2) | 32m 3.4s | 58.25 Mbps | four receiving rows observed |
| IDMXL (native-r2) | 26m 57.3s | 69.27 Mbps | sequential |

The stated broadband plan is 80 Mbps, supplied by the user, not a calibrated measured limit. Payload rate uses decimal megabits per second. Browser-assisted transfers continue using the browser's transport and are not represented by this native-engine benchmark.

IDM 6.43.15.2 was inspected in the actual desktop interface: four connections, no site exceptions, no download quota. For this fresh attempt, early downloading while the File Info dialog is open was temporarily disabled before entering the URL. Timing starts immediately before Start Download; Last Try Date matched that start to the displayed second. The original option was restored immediately after the transfer started. The destination was verified in the real interface. This avoids giving IDM an uncounted prebuffer or charging it automation time spent selecting a folder. Its metadata lookup occurred before the timer; IDMXL's probe remains inside its timer.

Completion includes each app's transfer, waiting, final processing and publication. IDMXL retains its built-in full-file verification. The IDM monitor waits for the expected-size file with an exclusive read handle; the native monitor waits for successful process exit. Polling adds up to roughly half a second, and UI timestamp precision is limited. Separate independent full-file SHA-256 and every ZIP-entry CRC check happened outside timing. Both files matched **73f98f72b4eb42b7a644435de20e53e626fe2947d1b21f17dec7f72fed51e28d**, all seven ZIP entries passed, neither archive was extracted or executed, and both owned payloads were deleted. The reference hash was independently verified locally; it is not a publisher-signed checksum.

## What changed

- Keep a verified Windows staging file and its exact saved-block map owned across routine server cooldowns. Retain incremental whole-file hashing too. A separate per-job lease prevents another worker from taking the same job while the queue allows unrelated jobs to proceed. The retained-session cache is bounded.
- Preserve normal validation after process restart, user pause/cancel, changed remote identity, changed saved extents or cache eviction. Windows sharing protection prevents other processes from modifying or replacing a retained staging file. Integrity checks were not removed.
- Remember repeated server throttling across successful progress. Increase request spacing, temporarily reduce concurrency when rejection persists, and relax gradually after sustained success. Retry-After deadlines remain respected.
- Group automatic large-file requests up to 128 MiB while keeping 1 MiB recovery blocks and a bounded 16 MiB writer queue. A single eligible queued file no longer tears down its transfer every 30 seconds; rotation still happens when another job needs a turn or a schedule closes.

The runtime is frozen at `0bfe65c3ee2e3c326be7fe871d10495b5293b740`. Application code and binaries were unchanged during these measured runs; the final documentation was assembled afterward. Schema 7 and extension 0.1.6 are unchanged by this speed update.

## Measured native phases

Latest native run: transfer loop **1536.372s**, saved-data validation **0.000s**, final verification tail **80.323s**, other/waiting **0.026s**. Recorded body requests: **1**; failed requests: **0**; recovery schedules: **0**; validated-session reuses: **0**; bytes rescanned for resume: **0**. Nested disk/checkpoint counters overlap other phases and must not be added to total elapsed time.

## Controlled check of the original bottleneck

A separate loopback server serves 1 GiB+257 bytes and rejects the final portion three times. Both old and new binaries use one connection and explicit 4 MiB requests, isolating retained-validation behavior from the larger automatic-request change. Order was old/new/new/old; each run used fresh state, verified the entire output independently, avoided refetching saved prefix bodies and deleted its owned payload. This is a local mechanism test, not an internet speed prediction or an IDM comparison.

| Run | Total seconds | Saved-data validation seconds | Session reuses |
| --- | --- | --- | --- |
| 1: previous | 84.169 | 15.354 | not instrumented |
| 2: candidate | 63.336 | 0.014 | 3 |
| 3: candidate | 64.221 | 0.011 | 3 |
| 4: previous | 84.238 | 14.195 | not instrumented |

Mean elapsed time: previous **84.204s**, candidate **63.779s**, or **24.26% less time**. Retain the individual results: the small sample and operating-system cache variation limit generalization. The candidate reused the validated 1 GiB prefix three times per run, avoiding 3 GiB of repeated saved-data scans. The new checks also verify that process loss or changed identity still causes validation instead of unsafe reuse. An earlier first-run attempt was interrupted after three rejections; it is excluded, its logs were preserved, and its owned 1 GiB partial file was removed before this fresh four-run sequence.

## Earlier observations retained

The preceding recovery build completed the same 14 GB source in **62m43.2s** versus IDM's **29m22.3s**. Its recorded costs included about 12m 06s of saved-block rescans and 20m 22s of waiting. That result remains in RECOVERY-RESULTS.md.

The first run of this candidate, on 22 October, completed in **29m 56.5s (62.36 Mbps)** with one sequential HTTP/2 body request and no failures. It passed independent SHA/ZIP checks and was deleted. Both recent native public runs selected sequential mode, unlike the earlier ranged run, so their improvement cannot be attributed to the range/session changes alone. The controlled fixture provides direct evidence of that mechanism. The 23 October pair is reported separately. The interrupted IDM setup from 22 October produced no valid completion result and is excluded, with its logs preserved.

## Reliability and update qualification

**219 distinct checks passed:** 68 Rust, 76 ordinary Python, one separately enabled >4 GiB resource case, and 74 Node. Formatting, warnings-denied lint, desktop/frontend and Windows installer builds pass. [Windows CI 39779365305](https://github.com/GeniusDevam/IDMXL/actions/runs/39779365305) passed for the exact frozen runtime commit. Focused reruns are not counted twice.

The **21,474,836,737-byte** generated test passed pause after 5 GiB, deliberate owned-process termination after 10 GiB and fresh-process resume in generation 1. It reused **10,854,858,752 saved bytes** without body refetch. An independent full published-file SHA matched `cc0d8604c1365e10356db6feb0113094da719f1d63c189a8a11b58c143b03030`; database integrity passed. A separate 4,294,967,553-byte test passed a 397-character Unicode path and exact engine hash against the generated reference. Owned payloads were removed.

An isolated updater preserved completed and paused history, kept a schema 6 backup before migrating to 7, and resumed saved bytes in the same generation. A controlled failure after migration kept readable upgraded history with its matching new engine. The matching preview is prepared; **the user's usual installed copy has not been updated**.

Initial default-sandbox tests could not reach even loopback, with zero server requests for both old and new binaries; tests with authorized networking passed. A default-temp publication access failure was resolved by using an owned workspace test-temp folder. These environment failures are retained in the logs. The regenerated CycloneDX inventory validates with zero schema errors; it is an inventory, not legal clearance for distribution.

## Limits and delivery

A 5% practical tolerance was declared before the public tests. It is an engineering acceptance band, not statistical confidence. One fixed-order pair does not control CDN routing, server load, ISP variation, cache state or unrelated household traffic. No universal superiority claim is made, and no proprietary IDM implementation was inspected.

The app remains an unsigned development preview. Normal-profile installation, signing/hosted updates, clean-machine distribution testing and broader site qualification are separate from these tests. No existing release was replaced or pull request merged. Full numeric evidence is in `evidence/speed-update.json`; older results remain intact.

Engine SHA-256: `b698214fe691ca39b5ececbb36d1af980f12540e4dba1e6fa416225bfa49d10e`. Matching desktop: `453a8df17010c93d3bd0fc5dce26064016e845c06cc53e733c66fa6d4f5f3aaa`. Installer: `ca9045da8fc13bb65e94d2b4edf1b0931184a4376d89704c704ac11100814e00`.
