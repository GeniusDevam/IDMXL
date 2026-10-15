# Dependency and distribution record

Cargo.lock is authoritative for exact selected versions. Rust 1.99.0 is pinned in rust-toolchain.toml. This milestone uses Rust/C tooling and Python only for tests; Python is not a shipped runtime.

Direct components: anyhow (MIT/Apache-2.0), clap (MIT/Apache-2.0), curl/curl-sys (MIT bindings with curl's own licence), ctrlc (MIT/Apache-2.0), fs2 (MIT/Apache-2.0), rusqlite (MIT) and bundled SQLite (public domain), serde/serde_json (MIT/Apache-2.0), sha2 (MIT/Apache-2.0), url (MIT/Apache-2.0), uuid (MIT/Apache-2.0), windows-sys (MIT/Apache-2.0), and tempfile (MIT/Apache-2.0, test-only).

Native transfer dependencies include libcurl, zlib and nghttp2. Windows Schannel and DPAPI are platform facilities under Microsoft's terms. Unicode/IDNA and other transitive notices must be captured from the actual graph. Cargo metadata and upstream licence files need a complete audit before distribution; this summary is not a licence clearance.

Before public release: generate an SBOM; check every SPDX expression, bundled native source, compiler runtime and installer component; produce notices and matching source/build materials where required; record signing and distribution decisions. Do not ship yt-dlp/FFmpeg/other media helpers until their exact builds and notices have been qualified.

Official API references consulted while implementing:

- https://docs.rs/curl/0.4.51/curl/
- https://docs.rs/curl/0.4.51/curl/easy/struct.Easy2.html
- https://docs.rs/curl/0.4.51/curl/multi/struct.Multi.html
- https://www.sqlite.org/pragma.html#pragma_synchronous
- https://tauri.app/start/prerequisites/

