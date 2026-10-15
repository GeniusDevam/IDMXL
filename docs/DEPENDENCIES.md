# Dependency and distribution record

Cargo.lock is authoritative for exact selected versions. Rust 1.99.0 is pinned in rust-toolchain.toml. This milestone uses Rust/C tooling and Python only for tests; Python is not a shipped runtime.

Direct components: anyhow (MIT/Apache-2.0), clap (MIT/Apache-2.0), curl/curl-sys (MIT bindings with curl's own licence), ctrlc (MIT/Apache-2.0), fs2 (MIT/Apache-2.0), rusqlite (MIT) and bundled SQLite (public domain), serde/serde_json (MIT/Apache-2.0), sha2 (MIT/Apache-2.0), url (MIT/Apache-2.0), uuid (MIT/Apache-2.0), windows-sys (MIT/Apache-2.0), and tempfile (MIT/Apache-2.0, test-only).

Native transfer dependencies include libcurl, zlib and nghttp2. Windows Schannel and DPAPI are platform facilities under Microsoft's terms. Unicode/IDNA and other transitive notices must be captured from the actual graph. Cargo metadata and upstream licence files need a complete audit before distribution; this summary is not a licence clearance.

Before public release: generate an SBOM; check every SPDX expression, bundled native source, compiler runtime and installer component; produce notices and matching source/build materials where required; record signing and distribution decisions. Do not ship yt-dlp/FFmpeg/other media helpers until their exact builds and notices have been qualified.

The desktop milestone adds Tauri 2.12.4, tauri-plugin-dialog 2.8.3, React/React DOM 19.3.1, TypeScript, Vite 8.3.4 and their transitives. Its separate Rust graph is locked in apps/desktop/src-tauri/Cargo.lock; npm versions and integrity hashes are locked in apps/desktop/package-lock.json. The save-dialog plugin also introduces a filesystem plugin transitively; the frontend is not granted general filesystem permissions. Prettier 3.6.2 was used as a temporary development formatter and is not a shipped runtime.

Official API references consulted while implementing:

- https://docs.rs/curl/0.4.51/curl/
- https://docs.rs/curl/0.4.51/curl/easy/struct.Easy2.html
- https://docs.rs/curl/0.4.51/curl/multi/struct.Multi.html
- https://www.sqlite.org/pragma.html#pragma_synchronous
- https://tauri.app/start/prerequisites/

The queue/protocol milestone adds httpdate 1.0.3 (MIT/Apache-2.0), tray-icon 0.25.1 via Tauri's tray feature, and static_vcruntime 3.0.0 for the CLI build script. The CLI and desktop use static VCRuntime linkage with Windows' UCRT. This reduces runtime-install prerequisites, but Microsoft compiler/runtime terms still require review. Test-only h2/hpack/hyperframe/cryptography versions are pinned in tests/requirements.txt. PGPy was used only as a development source-signature verifier, with limitations recorded in MEDIA-BUILD.md.

`scripts/sbom.ps1` collects the actual locked Windows Rust graphs, npm lock graph, linked engine native versions, observed NSIS tooling and Windows/WebView2/runtime prerequisites into a CycloneDX 1.6 inventory. It copies 516 root notice files for the current graph and flags 44 missing-root texts; it does not infer licence incompatibility or silently mark them cleared. The current inventory contains 353 components including build/test and optional-platform packages. Embedded native source notices and an artifact-specific runtime inventory still need audit.

The controlled FFmpeg helper experiment has its own exact-source/compiler/configuration record in MEDIA-BUILD.md. It is not bundled in the Core installer. No Python interpreter, extractor runtime or downloaded media helper is shipped by this milestone.

