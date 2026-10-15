# Initial architecture decisions

## ADR-001: keep the planned engine stack

Rust owns state, transport and storage. libcurl is statically built with Schannel and nghttp2 on Windows; SQLite is bundled. The `doctor` command reports linked capabilities. HTTP/2 capability is present but actual HTTP/2 negotiation has not yet passed a fixture.

## ADR-002: make replay an explicit user choice

Unknown URLs default to a single sequential request. `--replayable` permits one bounded range probe and subsequent requests. Ranged mode requires a strong quoted ETag, known total and exact Content-Range. Every extent request uses If-Match. A changed/ignored/malformed response fails before its body is written. A subsequent run probes again; incompatible identity creates a new generation and discards old extents.

Redirects are followed explicitly with the URL parser, restricted to HTTP(S) and without HTTPS-to-HTTP downgrade. Proxies inherited from the environment are disabled for now. No cookie or user-header forwarding exists. CLI URLs are explicit user inputs, so loopback/intranet downloads are possible; browser-origin requests must not be enabled until their separate network policy is implemented.

## ADR-003: durable extents and publication intent

Each fixed 1 MiB extent has a generation, offset, length and SHA-256. Network callbacks write positionally to a unique staging file beside the final destination. Completed extents are flushed before SQLite records them. An exclusive profile lock prevents two transfer engines; short CLI control transactions can still enqueue or request pause while the engine runs.

Resume verifies the local hashes of recorded extents. Partial in-flight extents are fetched again. Sequential jobs safely restart after interruption. New extents are scheduled lazily with a bounded retry queue; no file-sized buffer or full pending-file index is used. Recovery currently loads saved-extent metadata and must receive a compact/paged representation before very-large-file qualification.

Final verification records a publication intent with expected size, computed hash and Windows file identity. Zone.Identifier is written before a no-replacement MoveFileExW operation. Completion is committed afterward. Recovery requires matching file identity and hash, so an unrelated identical destination is not mistaken for the renamed staging file. Public scanner execution is not implemented.

## ADR-004: actual scope of the first milestone

One active file job per profile, configurable fixed range concurrency, per-job speed limit, bounded transient range retry, redacted history and safe file completion are implemented. There is no adaptive tuning, persisted retry queue, global scheduler, installer, updater, media downloader or browser capture yet. Zero software licence fees do not establish binary distribution compliance; see DEPENDENCIES.md.

## ADR-005: local secrets and source provenance

Windows source URLs are encrypted using per-user DPAPI. Query strings/fragments are omitted from display/history. Token-bearing paths are not supported as a secret-storage workflow. Use stdin rather than process arguments for signed URLs. The adopted PDF and Markdown plan are retained in docs, with a recorded SHA-256, and do not override newer direct user instructions.

Source PDF SHA-256: `FDCF0AB97B20C578217009CF744133E0D0797EBCE2F20328B2B79875144C0B85`.

## ADR-006: desktop lifecycle

The Tauri window invokes narrow add/list/control/start commands against the same store and launches a fixed CLI executable for transfer work. URLs do not enter the CLI process arguments. Windows CREATE_NO_WINDOW hides the helper console; dropping/closing the parent window does not intentionally terminate the engine process. A reaper thread closes process handles while the window is alive. History polling reconciles orphaned active states only after acquiring the profile lock.

The frontend has local assets, no remote executable pages, a restrictive CSP and only the native save-dialog permission beyond Tauri's default core capability. In debug builds the engine path is fixed to the current checkout; release builds look beside the desktop executable. Installation and helper authentication/signing remain release qualification work. The UI shows durable bytes; in-flight byte/speed events are a next-step contract.

