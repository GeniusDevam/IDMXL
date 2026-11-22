# IDMXL recovery implementation and retest

22 October 2026. Runtime commit `eb087afaf4011a73f2ba6ea1778d033ccae28abf`; extension **0.1.6**, database schema **7**. The prepared update has **not** been applied to the normal-session installation.

Both fresh 14 GB transfers completed and passed independent verification. IDMXL took 113.54% more elapsed time in this pair. This is one-source evidence, not a general speed ranking.

## Implemented changes

- HTTP 429/503 puts reusable native downloads into persisted **Waiting for server**, preserving saved blocks and resuming automatically while their worker or queue is running.
- Retry-After seconds and dates are honored, including waits beyond five minutes. Missing/invalid instructions use increasing 5–300 second waits with a small random variation.
- Throttling uses a separate no-progress allowance, default **24 hours**, configurable up to seven days. It no longer exhausts the small transport retry budget. Pause and Cancel remain effective during waiting.
- Per-origin request pacing slows after rejection and gradually relaxes after success. Large automatic body requests retain a 4 MiB minimum near completion, except the final remainder; 1 MiB recovery checkpoints remain.
- Numeric diagnostics retain exact status counts and bounded response/recovery histories. The desktop and browser extension display waiting and expose its controls.
- The migration saves the old schema first. The prepared updater keeps updated binaries and upgraded history together if verification fails after migration.

## Validation

**213 unique local checks passed:** 66 Rust, 72 ordinary Python, one enabled >4 GiB resource test, and 74 Node checks. Focused reruns are not counted twice. Formatting, warnings-denied root/native Clippy, frontend production build and full NSIS packaging passed. [Windows CI](https://github.com/GeniusDevam/IDMXL/actions/runs/39722100156) passed for the exact runtime commit.

Real generated HTTP fixtures exercise repeated 429/503 at the tail beyond the old two-retry limit, absent/invalid/date Retry-After, persistent ten-minute deadlines across restart, recovery expiry, Pause/Cancel, another origin proceeding, waiting snapshots and throttled media segments. Long-deadline persistence tests advance an isolated stored deadline; they do not claim a ten-minute wall-clock wait. Other fixtures exercise real waits.

The separate 4,294,967,553-byte resource transfer passed through a **397-character Unicode path**, with engine SHA-256 matching the independently generated reference, exact published size and payload deletion. The 21.47 GB test below additionally rereads and hashes the full published file independently.

### Controlled 21.47 GB recovery transfer

Transferred **21,474,836,737 bytes** of distinct generated offset patterns. Paused after 5 GiB, deliberately terminated only its engine after 10 GiB, and resumed in a fresh process. Generation remained unchanged; **10,780,409,856 saved bytes** were reused without requesting their body ranges again. SQLite integrity check passed. The independent full disk SHA-256 matched:
`cc0d8604c1365e10356db6feb0113094da719f1d63c189a8a11b58c143b03030`.

### Fresh public 14 GB comparison

Source: [official Blender Tears of Steel DCP archive](https://download.blender.org/demo/movies/ToS/ToS_4k_DCP.zip), **14,004,054,153 bytes** each. Order: IDM, then IDMXL, on the same PC. The actual IDM interface showed a four-connection limit with no site exceptions; IDMXL used the same ceiling. No IDM setting was changed. The 80 Mbps service rate is user supplied, not a calibrated measurement.

| Application | Result | Elapsed time | Average payload rate | Independent file checks |
| --- | --- | ---: | ---: | --- |
| IDM | Completed | 29 min 22.3 s | 63.57 Mbps | SHA-256 and all ZIP CRCs passed |
| IDMXL | Completed | 62 min 43.2 s | 29.77 Mbps | SHA-256 and all ZIP CRCs passed |

IDM had **Start downloading immediately while displaying Download File Info** enabled. Its time therefore starts at the observed **04:47:45 UTC Date Added/Last Try Date**, with one-second timestamp precision, and includes transfer before the later Start click. Native timing begins at worker launch and includes final verification/publication. These are application lifecycle timings; synchronized packet-level timing was not captured. Independent SHA scanning and ZIP CRC checks occur after the reported completion times.

Two background IDM CLI handoffs created no transfer or output; both were excluded before the interface-started transfer. IDM initially ignored an accessibility-field destination edit. Its active destination was corrected through its normal chooser without pausing or restarting; the timing includes that period. The unchanged-candidate harness retained these setup observations.

Both completed copies have SHA-256 `73f98f72b4eb42b7a644435de20e53e626fe2947d1b21f17dec7f72fed51e28d`. There is no separately supplied publisher checksum; matching independent hashes and archive CRCs establish consistency, not signed publisher authenticity.

IDMXL admitted **921 transfer requests**. Its completed-body response counts were `{"206": 841, "429": 20}`, including **20 real HTTP 429 rejections**. The recovery log contains **22 schedules**, also covering probe-stage throttling. Request admission, completed responses and recovery schedules are different counters; requests canceled in flight after a rejection do not all produce completed response records. The download nevertheless completed without a manual retry.

### Where IDMXL spent its time

| Recorded phase | Time |
| --- | ---: |
| Transfer loop, including its storage/checkpoint work | 29 min 09.9 s |
| Saved-block validation across recovery attempts | 12 min 06.0 s |
| Remaining verification tail | 0 min 58.5 s |
| Probes | 0 min 06.1 s |
| Other elapsed time, dominated by cooldowns | 20 min 22.3 s |

The recorded deadlines requested **20 min 20.3 s** in total. That is consistent with the approximately 20-minute other/waiting category; it is not an independently instrumented pure-wait counter. The **12-minute saved-block validation cost** comes from reopening and checking previously saved data on successive recovery attempts. These two categories account for most of the roughly 33-minute gap. Nested disk/checkpoint timings overlap the transfer loop and must not be added to this table. IDM's corresponding internal phases and HTTP responses were not instrumented.

The next speed priorities are to learn a sustainable request rate from repeated recent throttling, even when some data continues to arrive, and to avoid repeated full saved-block scans within one continuously owned worker. The latter needs an exclusive handle and a proven no-change condition; full validation must remain after restart or any possible modification. These are follow-up optimizations, not changes silently introduced into this measured build. Retry-After and integrity checks should remain intact.

## Interpretation and limits

The previous public run stopped at 99.91534% with server_throttled after exhausting a per-block retry budget. That failure remains in the earlier report. This implementation addresses that mechanism without weakening identity or checksum validation. The new public test also uses a four-connection ceiling instead of the earlier eight, so a changed result cannot be attributed solely to the recovery code.

This is one pair from one public source, in a fixed order. Server load, CDN/cache state, ISP behavior and unrelated traffic can affect rates. The controlled local test establishes size/recovery correctness, not broadband acceleration. No universal advantage over IDM or full IDM feature parity is established. Browser-assisted transfers continue to use the browser network engine. Physical power loss, broader storage targets, a new real-browser waiting-control click-through and production signing remain outside this qualification.

## Update, cleanup and evidence

The isolated update qualification preserved history IDs and a completed file, upgraded schema 6 to 7 with a schema-6 backup, and resumed a partial file in the same generation without fetching saved body blocks. A deliberate failure after migration retained a readable upgraded database and its matching new engine. These checks did not access normal IDMXL history.

The first package qualification caught a path-separator mismatch before replacing its isolated installed files. The package was corrected and both final cases passed. Initial HTTP test teardown errors were caused by unclosed Python test connections; those handles were fixed and the full suite passed. These setup observations are preserved and are not counted as successful engine transfers.

The regenerated dependency inventory contains **379 components and 561 notice files**, with zero official CycloneDX 1.6 schema errors. The existing 44 root-notice review items are not legal release clearance.

**All large completed/generated payloads and owned partials were deleted after verification.** The isolated update payloads were also removed. State, numeric diagnostics, reference hashes and cleanup receipts remain for audit. The archives were not extracted or executed.

Prepared package: sibling **IDMXL-Recovery-Preview**. Use its START-HERE.md and Finish-IDMXL-Update.cmd from the normal File Explorer session to apply it. Tests did not replace the usual installed app or re-register its browser host. No pull request was merged and no binary release was published.

Engine SHA-256: `6e9df870469d891244c52cf0af0ef5a1deaa1bf69ec670d365bf3e310ceced86`. Desktop SHA-256: `3d40f8bf7c948d3993b8e86237b079e75ce85c6fa35602ea77bcafbb2258a4c4`. Installer SHA-256: `036b03beb3fbeb123f4fefcde17bcfd54db992198ceb2a5d32463fd3a6519ba1`.

Machine-readable summary: [evidence/recovery.json](evidence/recovery.json).
