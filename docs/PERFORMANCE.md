# Download performance

The engine fetches reusable HTTP(S) files with conditional ranges and verifies
the resulting file before publication. Performance depends on the source,
network and storage; acceleration is conditional. Browser-assisted transfers use
the browser's transport and do not receive these native-engine optimizations.

## Transfer pipeline

- Recovery extents remain **1 MiB**, preserving existing saved downloads.
- For files at least 32 MiB, HTTP requests group extents into **1–16 MiB leases**.
  The scheduler shrinks leases as the remaining work decreases and never spans
  an already verified saved extent. Smaller files retain 1 MiB requests.
- Receive callbacks enqueue validated bytes in a **16 MiB bounded queue**. A
  storage worker coalesces positional writes into at most **128 KiB per active
  range**, computes block hashes, and announces only completely written blocks.
  When the queue fills, libcurl is paused and later resumes the unaccepted
  callback exactly once. Extra memory used by TLS/HTTP libraries and operating
  system socket buffers is separate from this owned-queue bound.
- Completed extents are checkpointed together after **32 MiB** or **2 seconds**
  for files at least 32 MiB; smaller files use **8 MiB** or **500 ms**. The
  worker checks the thresholds between queued messages, so a threshold can be
  exceeded by the last message. Pause, transfer failure and normal completion
  flush the remaining complete extents. Partial blocks are fetched again.
- Every durable batch follows **write → data flush → FULL SQLite transaction**.
  Checkpoint totals use idempotent deltas, rather than rescanning all extents.
- Writes and checkpoints run on the storage worker, allowing network reception
  to continue through short disk stalls. Per-request end markers precede retry
  data in the same FIFO, so old incomplete buffers cannot overwrite a retry.
  Pause/cancel drain accepted work, save complete blocks and join both workers;
  they never publish an unfinished destination. A slow or failed disk still
  limits total completion time once bounded buffering is exhausted.
- A separate SQLite connection uses NORMAL synchronization for transient
  speed/received/ETA updates and request-start counters. Admission obtains writer
  ownership before reading the latest committed retry budget. Failed-retry
  budgets, recovery extents, identity and publication keep FULL synchronization.
  Recovery resets transient progress; a power loss can roll back recent start
  counters, but cannot replenish a previously durable failed-retry budget.
- An independent read handle hashes the growing completed prefix in a worker
  thread. Standard whole-file SHA-256 is retained; chunk digests are not combined
  as a substitute. Publication waits for the completed file, data flush, final
  digest comparison and publication-intent transaction.
- On Windows, the active staging writer permits other readers but excludes
  external writers/deletion. The verifier checks file identity before starting.
  It owns its read cursor and joins on success, pause and error.
- Available request slots are refilled immediately. Idle retry waits remain
  bounded, and active requests still respect persistent origin Retry-After.

Larger checkpoint batches trade fewer storage barriers for more refetching
after an abrupt exit. A process exit can lose recent uncommitted progress; saved extents are
never advertised as durable before their data flush. Saved blocks are hashed
again on resume. Hardware power-loss behavior remains a separate qualification.

## Connection policy

Fixed/adaptive ceilings support **1–8 concurrent requests**. Existing saved
settings retain their values; new reusable jobs still default to a ceiling of
four. Actual TCP connection count can differ under HTTP/2 multiplexing.

Automatic admission evaluates pairs of one-second goodput samples. Successful
trials explore 1 → 2 → 4 → 8, bounded by the selected ceiling. A trial needs at
least 10% improvement; unsuccessful trials return to the preceding limit with
a cooldown. Active leases drain normally. Throttling reduces admission to one.
Rate-capped jobs retain fixed admission and divide the cap among active slots.

HTTP/3, persistent learned origin/network profiles, cross-origin mirror
aggregation and mid-flight speculative work stealing are not enabled by this
change. They require separate evidence and correctness qualification.

## Diagnostics and controlled experiments

For a native file job:

```powershell
.\target\release\idmxl.exe run <job-id> --connections 4 --metrics .\run-metrics.json
```

The metrics destination must be a new file. Metrics contain numeric phase costs,
request and checkpoint counts, newly created connection counts and HTTP version
counts. They omit URLs, source tokens, cookies and output filenames. Connection
and protocol information covers finished body requests; interrupted active
requests may be absent. Probe timing is separate.

`download_ms` excludes resume validation and the remaining verifier wait.
Checkpoint, storage, callback and request-accounting costs are sub-costs of
downloading and may overlap across threads; do not add them to the phase total.
`callback_write_ms` records enqueue work for asynchronous range transfers;
`storage_write_ms`/`storage_hash_ms` record worker activity and
`storage_drain_ms` records the remaining writer wait at the end. Queue peak and
backpressure counters permit bounded-buffer qualification.
Per-request DNS/connect/TLS/first-byte fields
are sums of libcurl's cumulative request timestamps, can overlap across requests,
and must not be interpreted as an additive wall-time breakdown. Diagnostics add
measurement overhead and should also be compared with an uninstrumented run.

Optional CLI-only experiment controls:

```powershell
# Group up to 8 recovery blocks per HTTP request.
.\target\release\idmxl.exe run <job-id> --request-mib 8

# Request HTTP/1.1, or attempt HTTP/2 over TLS with fallback.
.\target\release\idmxl.exe run <job-id> --http-version http1
.\target\release\idmxl.exe run <job-id> --http-version http2
```

These preferences affect body requests. The initial safe probe retains its
established negotiation and identity checks. `--request-mib` accepts 1–32; it
does not change recovery-block size. Diagnostic output is for native file jobs,
not multi-component media parents.

## Reproducing the engine comparison

Keep a byte-identical copy of the previous release and build the candidate in
release mode, then run:

```powershell
python .\scripts\benchmark-performance.py --baseline C:\bench\baseline.exe --candidate .\target\release\idmxl.exe --output .\.local\performance-run --repeats 3 --warmup
```

The output directory must not already exist. The script alternates randomized
paired runs over generated, block-distinct local data. Every result is checked
against the reference digest before its owned payload is individually removed.
It retains isolated profiles, raw replies, metrics, binary hashes and summaries.
It never opens the normal application history. `--warmup` retains one excluded,
verified generated warm-up per engine. Repeat `--case <name>` to select local
cases when investigating a specific result.

`--public` additionally downloads the fixed 399 MB Blender test ZIP per engine
and repetition, using its pinned publisher SHA-256. `--only-public` runs that
case alone. Three public pairs consume about 2.4 GB of payload. The ZIP is never
executed. This compares two IDMXL engines; it does not establish a win over IDM.
Local rate settings are fixture ceilings until independently calibrated.

Required qualification includes old-profile resume, grouped-request truncation,
retry-budget persistence across regrouping/restarts, wrong hashes, storage
errors, TLS/HTTP2, pause/cancel, and abrupt exits around data/metadata/publication
barriers. A broad IDM superiority claim requires contemporary multi-origin
paired product measurements, not a comparison with an earlier day's IDM time.
