# IDMXL performance implementation and validation

Latest qualification: the [22 October recovery retest](RECOVERY-RESULTS.md)
completed a verified 14 GB file after repeated real server rejections, but took
62m43s versus IDM's 29m22s. The older 399 MB observation below is a separate,
limited sample and does not establish general superiority or large-file parity.

21 October 2026. Measurements use the frozen optimized engine listed below. This is a preview qualification, not a general claim of superiority over IDM.

## Direct comparison with IDM

| App | Mean / median / range (seconds) | Verified files |
| --- | --- | --- |
| IDM | 55.572 / 54.208 / 49.464–63.045 | 3/3 |
| IDMXL | 48.245 / 48.478 / 47.673–48.583 | 3/3 |

IDMXL used **13.19% less mean completion time** in this specific comparison. This result covers one source and three runs per app. It does not establish the proposed multi-origin release target.

The test file was the official Blender Windows ZIP: 399,051,129 bytes. Each of six serial downloads matched the pinned published SHA-256. Order: IDM, IDMXL, IDMXL, IDM, IDM, IDMXL. Each verified test ZIP was individually removed; none was executed. Payload traffic was 2,394,306,774 bytes (about 2.39 GB), plus protocol overhead.

IDM saved connection setting: **8**. IDMXL requested eight concurrent ranges, with two retries. IDM settings were not changed. This tests an explicit eight-request configuration, while everyday automatic defaults retain a ceiling of four. Request limits do not prove equal physical sockets; HTTP/2 can multiplex.

Timing starts at command launch and ends when IDM exposes the full file with an exclusive read handle, or when IDMXL exits successfully with that full file. Polling is at 100 ms. A separate external hash read is excluded from both timings; IDMXL retains its built-in whole-file verification. IDM uses its no-question/exit-after-transfer command options; IDMXL starts a fresh CLI engine for each run. Internal lifecycle and verification work differ. CPU, whole-product memory and wire-level sockets were not measured.

IDMXL used 61 body requests per public file instead of the original engine's 381 one-block requests. Its measured final verification wait ranged from 0.136 to 2.073 ms. SHA-256 still reads the entire file; most of that work now overlaps downloading. Storage costs overlap network reception and must not be added to total time.

The diagnostics still show storage stalls and repeated backpressure callbacks. Owned queue bytes stay bounded, but whole-product CPU and message-queue wakeup behavior need further profiling. No additional tuning was made against this final evaluation set.

## Optimized engine versus original IDMXL

Each case uses 64 MiB of block-distinct generated data, three serial randomized pairs, four requested ranges and the same destination volume. One excluded warm-up per engine is verified. The final executable was frozen before evaluation. A single laptop hosts both server and client; shaping rates are configured ceilings rather than a calibrated broadband link.

| Case | Original mean / median / range (s) | Optimized mean / median / range (s) | Mean time reduction |
| --- | --- | --- | --- |
| unlimited | 3.218 / 1.735 / 1.221–6.699 | 0.669 / 0.665 / 0.658–0.683 | 79.22% |
| request-latency-30ms | 2.480 / 1.880 / 1.571–3.987 | 1.654 / 0.697 / 0.692–3.573 | 33.29% |
| shared-configured-80mbps | 10.169 / 11.017 / 7.356–12.134 | 6.852 / 6.848 / 6.843–6.864 | 32.62% |
| per-request-1mbps | 24.538 / 21.492 / 17.637–34.484 | 18.308 / 17.501 / 17.018–20.404 | 25.39% |
| adaptive-per-request | 24.758 / 22.666 / 22.550–29.059 | 20.062 / 20.059 / 20.046–20.080 | 18.97% |

All 30 measured files passed full digest verification, with zero failed body requests. Body request count fell from 64 to 21 per file. Automatic mode is explicitly selected only in its named case; other cases use fixed concurrency. All slow samples remain included. These small samples describe this setup; no population confidence or multi-origin claim is inferred.

The candidate has numeric instrumentation enabled; the original binary has no equivalent option. A separate three-pair configured-80-Mbps test uses the same final binary on both sides, with diagnostics disabled/enabled respectively. Results:

```json
{
  "shared-configured-80mbps": {
    "baseline": {
      "mean_seconds": 7.400164033344481,
      "median_seconds": 7.674851700023282,
      "runs": 3,
      "verified": true,
      "mean_requests": 21
    },
    "candidate": {
      "mean_seconds": 6.834328066674061,
      "median_seconds": 6.834053600003244,
      "runs": 3,
      "verified": true,
      "mean_requests": 21
    },
    "candidate_time_reduction_percent": 7.6462624898693266
  }
}
```

This estimates instrumentation exposure under the local fixture and ordinary machine variance, not a universal correction factor. No correction was subtracted from the reported download times.

## Changes implemented

- Variable 1–16 MiB HTTP leases for larger files; compatible 1 MiB saved recovery blocks and canonical persistent failed-retry budgets.
- Immediate scheduler refill, incremental checkpoint accounting, and lightweight progress/request-start counter writes.
- A bounded 16 MiB receive queue and dedicated writer with 128 KiB per-range coalescing. Backpressure pauses libcurl without accepting the callback twice.
- File-data flush before each FULL recovery transaction. Large files checkpoint at 32 MiB or two seconds; small files at 8 MiB or 500 ms. Pause/error/completion drains and saves complete blocks.
- An independent disk reader computes ordinary whole-file SHA-256 while immutable prefixes become available. Publication still waits for durability and final verification.
- Bounded 1/2/4/8 adaptive trials, rollback/cooldown, saved desktop ceilings up to eight, numeric phase diagnostics and explicit experimental H1/H2 preferences.

A power loss can discard recent lightweight counters or uncommitted progress; durable failed budgets and extents retain FULL transactions. Larger checkpoint batches reduce barriers in exchange for more possible refetching after interruption.

## Correctness and delivery

- 199 unique local checks: 58 Rust, 66 ordinary Python, one separately enabled real resource case and 74 Node checks. Formatting, root/native Clippy, TypeScript/Vite and NSIS packaging pass.
- Real 4294967553-byte transfer, 397-character Unicode path, expected hash and no-overwrite publication pass. Transfer/verification: 123.88 seconds; full fixture: 137.344 seconds. This is not an internet speed result.
- Crash injection covers eight persistence/publication boundaries for both one-block and grouped leases. Slow-writer backpressure, queued pause/cancel, writer failure, changed resources, TLS/HTTP2 and bad hashes are covered.
- An isolated existing-install update preserved two history IDs, a completed file and 2,097,152 paused bytes. The new engine resumed in the same generation without refetching the saved body blocks; output hash and database integrity passed.
- One initial media recovery fault case returned 1 before the intended injected exit. Its assertion lacked stderr; that diagnostic is fixed. The seven-case media rerun and complete functional recheck passed. The initial cause remains unknown.
- The preview installer, matching app/engine update files, dependency inventory and 561 collected notices are prepared. Existing normal-session installation is not claimed updated. The public binary release was not replaced.
- [Windows CI](https://github.com/GeniusDevam/IDMXL/actions/runs/39589624438): completed, success for the tested code commit.

HTTP/3, persistent learned origin/network profiles, cross-origin mirrors and speculative work stealing remain separate experiments. Real power loss, new manual Chrome click-through, clean-machine installation and distribution qualification are not established here. Browser-assisted downloads continue using browser transport.

## Development measurements retained

Earlier v1/v2 runs were used to identify storage stalls and remain saved. They are not mixed into final-run statistics. All results, including regressions and outliers, are retained locally.

| Development build / case | Original mean (s) | Candidate mean (s) | Candidate median (s) |
| --- | --- | --- | --- |
| v1 / unlimited | 4.169 | 0.702 | 0.702 |
| v1 / request-latency-30ms | 3.142 | 0.740 | 0.726 |
| v1 / shared-configured-80mbps | 10.272 | 6.839 | 6.842 |
| v1 / per-request-1mbps | 25.251 | 18.657 | 17.575 |
| v1 / adaptive-per-request | 23.361 | 25.581 | 20.064 |
| v2 / unlimited | 1.914 | 1.834 | 0.672 |
| v2 / request-latency-30ms | 1.594 | 2.654 | 0.680 |
| v2 / shared-configured-80mbps | 9.065 | 6.828 | 6.828 |
| v2 / per-request-1mbps | 18.060 | 17.030 | 16.923 |
| v2 / adaptive-per-request | 22.618 | 20.061 | 20.062 |

## Provenance

- Source: `673a8d3ce569902996a1a034234095b03c9c36ee`; [review](https://github.com/GeniusDevam/IDMXL/pull/1).
- Original engine SHA-256: `1f1a8f747be468f941cdae6ece69f2c10f4c4e73184ba6e791ee9cc3fac707fd`.
- Update-Files/idmxl.exe: `ad51c56d30c4d835c4a49c2498aee8c5e610a49b162a9bdcc2d08877c0a0ceaf`.
- Update-Files/idmxl-desktop.exe: `666a57f990e3ee522545c74f7ae1ded123888d72facfa933a1aada3bbf5077f3`.
- IDMXL_0.1.0_x64-setup.exe: `ff4d889b982b70384a4abf8f78b88bf5f7d09f23b995a211ebaf2d0c317cc0ce`.

- Publisher test file: [Blender Windows ZIP](https://download.blender.org/release/Blender4.5/blender-4.5.9-windows-x64.zip); [published checksums](https://download.blender.org/release/Blender4.5/blender-4.5.9.sha256); SHA-256 `41da973b9bf95bb312cbeff4d1982feb13259b43c821686b9bafea4dfe5477cf`.
- Sanitized numeric runs, per-request phase metrics and qualification evidence: [performance.json](evidence/performance.json). Reproduction and design: [PERFORMANCE.md](PERFORMANCE.md).
