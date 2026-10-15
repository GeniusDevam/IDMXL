# Test evidence

## 15 October 2026: Windows x64 development machine

Rust 1.99.0, MSVC Build Tools 2022, bundled libcurl reporting 8.22.0-DEV with Schannel and HTTP/2 capability, nghttp2 1.68.1 and SQLite. Exact versions are recorded in Cargo.lock. The official rustup installer was downloaded into a workspace-local tools directory; no global PATH changes were required.

`cargo test --workspace --locked --offline`: 9 unit tests passed. Cases cover URL redaction/protocol rejection, device/stream filenames, DPAPI round trip, range bounds, changed resource identity, ignored ranges, publication recovery, rejection of unrelated identical files and conservative handling of long/unparsed Retry-After deadlines.

`python -m unittest discover -s tests -v`: 19 end-to-end tests passed in approximately 22 seconds after final engine changes. Cases cover parallel ranges, range-ignore/weak-validator fallback, unknown-length single request, manual redirects, changed resources, malformed ranges, unexpected encoding, truncated transfers, numeric Retry-After, pause/resume extent reuse, process-kill recovery, corrupted extents, raced destination collision, digest mismatch, engine ownership, cancellation, protected signed URLs and future database schema rejection.

`cargo clippy --workspace --all-targets --offline -- -D warnings`: passed. The restricted filesystem emitted incremental-cache hard-link warnings, unrelated to Rust/Clippy source diagnostics; compilation continued using copies.

Tests use generated ~3 MiB files served only on 127.0.0.1. Every successful result is independently hashed. On Windows, successful output is checked for ZoneId=3. CLI processes use isolated state and output directories.

Sandbox restrictions required the HTTP suite to run with loopback network access. Early sandbox-only runs were environmental failures and are not counted as successful evidence. A Python fixture database handle was explicitly closed to permit Windows cleanup.

No public-site compatibility, 8K media, real power-loss safety, >4 GiB transfer, HTTP/2 negotiation, installation, update, antivirus scanning, accessibility or acceleration performance claim is established by these results.

