# Release and recovery checks

This is a private development preview. An installer build, source repository or passing fixture does not make the Core release complete.

## Build and inspect

Run `scripts/test.ps1 -Faults -Protocols -Media`, native Clippy, and `scripts/build.ps1 -Bundle`. The NSIS output is under `apps/desktop/src-tauri/target/release/bundle/nsis`. It is a current-user Windows x64 installer with WebView2 detection/bootstrapper support. It bundles the engine, extension/setup guide, controlled media helpers, matching FFmpeg source archive, build recipe and notices. Python and the separate fixture encoder are not bundled.

`scripts/sbom.ps1` creates a resolved CycloneDX inventory, collected notice files and a missing-notice review under `.local/release-inventory`. It includes build/test graphs and externally provided Windows/WebView2/installer components. It must be narrowed/reviewed against the exact shipped artifact; missing root licence files are not automatically evidence of an incompatible licence. Platform/compiler runtime terms, embedded native notices and a product licence still need review before public distribution.

The preview is unsigned. Do not bypass SmartScreen or other Windows protections. Public Authenticode trust, update signing, release hosting and extension-store acceptance require separate release decisions. The updater implementation and setup tools exist, but no production authority is configured. UPDATES.md lists the owner-supplied Authenticode certificate, Ed25519 metadata key and HTTPS hosting.

## Upgrades and database recovery

Pause the queue and active transfers, wait for their checkpointed stop, and exit the interface from the tray before replacing app files. A background queue/engine can outlive the visible window. Existing-profile migration refuses to proceed while an engine owns its lock.

Upgrades from schema v1/v2 create `jobs-v1-<uuid>.sqlite` / `jobs-v2-<uuid>.sqlite` snapshots in the profile before changing that version. The snapshots include committed WAL data. Migration/future-version/idle-lock fixtures cover these transitions. Snapshot URLs remain protected for the same Windows user; do not treat them as a portable secret export.

For manual recovery, keep the original profile intact first. With all app/engine writers stopped, copy the matching snapshot into a separate recovery profile as `jobs.db`, without stale WAL/SHM files, and use the corresponding compatible application version. Retain referenced staging/final files. Resume verifies saved extents and source identity; a missing/changed partial may require a new generation. An old executable must not open a newer schema. These manual steps remain available for older snapshots. The updater also saves matching previous binaries and a protected receipt; update-recover validates it and creates a separate restored profile. See UPDATES.md. Trusted installation/recovery qualification remains open.

## Still required for Core

- Real Edge installation, context-menu handoff and fallback testing; store acceptance is independent.
- Clean Windows 11 x64 VM installation without developer runtimes; upgrade and broader uninstall preservation checks. One isolated development-machine silent uninstall passed on 16 October 2026.
- Real trusted update installation, certificate/key rotation and operator recovery rehearsal. Signature/freshness/replay and separate-profile recovery fixtures pass; they do not substitute for a trusted Windows update round trip.
- Physical storage power-loss, disk-full, permissions, reparse race, removable/network filesystem and sleep/network-change qualification.
- Wake/timezone-transition qualification, default/UI adaptive concurrency and broad benchmarks, remaining Core workflows and accessibility. Recurring queue hours and import folder rules have scoped fixture/native evidence in WORKFLOWS.md.
- Licence/notice closure and artifact-bound release SBOM. No speed advantage, full filesystem support or complete IDM parity is claimed.
