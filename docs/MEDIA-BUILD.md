# Controlled media helper builds

The first sections record the historical G0 experiment. The current bundled product build and hashes are recorded under Product layers below.

FFmpeg 9.0.2 was built from the official source with w64devkit 2.10.1 / GCC 16.2.0. The helper enables only local file I/O, selected demuxers/muxers/parsers and stream copy. No encoders or decoders are enabled; network, GPL, nonfree and version-3 components are disabled. `-version` records the full configuration; configure reports LGPL-2.1-or-later. This is a development feasibility experiment, not a bundled media feature or a redistribution clearance.

`scripts/media-build.ps1 -BuildDirectory <existing-or-new-folder>` downloads only the pinned archives and checks their SHA-256 values before extraction/build. The source hash was established after checking the detached release signature against FFmpeg's published key fingerprint `FCF986EA15E6E293A5644F10B4322F04D67658D8`. The verification used PGPy 0.6.0; it verifies the detached signature but warns that it does not implement full key self-signature/revocation policy. The fingerprint was checked against the official HTTPS download page. This limitation is retained in the release review.

| Input/artifact | SHA-256 |
|---|---|
| FFmpeg 9.0.2 source tar.xz | `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e` |
| w64devkit x64 2.10.1 archive | `add1edf984ce4d473cfeb7c52e0ab38db73a855bba6c7f564d5e16648eb2896a` |
| Observed ffmpeg.exe | `40418630042a2b8a57f2814000fb5a9f9ac9fb2ce4a134ab93b795c08e912b61` |
| Observed ffprobe.exe | `06efed650829241b6eb49fca11b409541ab59f95e13ee6c62d8abc1efd51196e` |

Observed binary hashes identify this build; they are not a reproducible-build claim. The initial Windows Make invocation emitted `@printf` diagnostics but compiled/linked both working binaries. The saved recipe uses `V=1` to avoid forwarding that echo prefix to the shell.

## Verified experiment

```powershell
python scripts/media-experiment.py --helper-dir <build-folder>\remux --output .local\media-experiment
```

The script generates one original 7680Ã—4320 RGB frame in a minimal MOV and stereo 48 kHz signed 16-bit PCM in WAV. It combines them into MOV using fixed `-c copy` arguments and a `file` protocol whitelist, then checks video geometry/codec, the encoded video packet SHA-256, and independently extracted audio samples. It passed on the development Windows x64 machine. The source is generated directly at 8K, with no upscaling or third-party media.

This qualifies only the stated MOV/RGB + WAV/PCM experiment. AV1/VP9/HEVC, MP4/WebM/MKV codec combinations, HDR metadata, adaptive manifests, format selection, helper lifecycle management and named extractor/site support remain Media-release work. The helper is deliberately not included in the Core installer. Its exact source, build recipe, notices and static compiler-runtime obligations must accompany a future helper distribution.

Primary sources: [FFmpeg source and release verification](https://www.ffmpeg.org/download.html), [FFmpeg legal/build obligations](https://www.ffmpeg.org/legal.html), [w64devkit release](https://github.com/skeeto/w64devkit/releases/tag/v2.10.1), [stream copy](https://www.ffmpeg.org/ffmpeg.html#Streamcopy).

## Product layers: 16 October 2026

The controlled helper is now bundled for the scoped workflow in MEDIA.md. Its current recipe enables the AAC decoder for stream information, while retaining no encoders/network/GPL/nonfree/version-3 components and stream copy for every assembly. The earlier G0 hashes above identify historical binaries only.

Current local build hashes:

- ffmpeg.exe: `292f03f3d20d990aa6c09c2c420f049a8c2690c7c965eb48f18e6354cd753f77`
- ffprobe.exe: `e1f6ee0a87b3bebfcad3a2c50a861006c8213a71cadc19f4459395fb3374a85f`

Stage-media validates the source hash and current recipe marker, embeds the exact helper hashes into the app/engine build, and packages source, recipe and notices. Binary hashes are build-specific; reproducible builds are not claimed. A separate test-only recipe enables AAC encoding solely to generate original fixtures and is never staged into the product. Product fixtures now cover original 8K MOV/PCM, HLS AAC byte ranges/audio-only, static DASH AAC/fMP4, source corruption, pause/restart and publication recovery. The wider streaming-codec/HDR and security/distribution gates remain open.
