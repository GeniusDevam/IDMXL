# Media downloads

Choose **Download media** in IDMXL, enter a direct file or a finished `.m3u8` / `.mpd` manifest, and select **Find original tracks**. Choose the exact video/rendition and matching audio language, then a compatible output container and destination. Add it to the ordinary queue or start it from history. Audio-only output is available; this extracts the selected audio without conversion. Separate direct video and audio URLs are supported.

The browser popup also offers **Video and audio in this tab**. This inspects media elements only after a click. It can transfer public HTTP(S) sources, not blob players or browser-bound sessions. It does not implement universal video-site discovery or bypass protection.

## Qualified implementation boundary

- Direct MOV/MP4, Matroska/WebM and supported audio inputs use the existing download engine. A direct unknown file receives one request without a probe. Explicit retry can restart it; it never splices uncertain bytes.
- HLS supports clear VOD, an explicit master rendition and external audio group, a single initialization map, TS/fMP4/AAC content and byte-range addressing. It rejects keys (including AES), discontinuities, gaps, low-latency parts, variable substitution and live playlists.
- DASH supports static, one-period, zero-offset presentations, one BaseURL at each level, SegmentList and bounded SegmentTemplate duration/Timeline forms. Templates support RepresentationID, Number and Time. Negative repeats, format specifiers, SegmentBase, multiple periods, protection and dynamic streams are rejected. Initial streaming fixtures use AAC/fMP4; broader codec/HDR matrices still require qualification.
- Byte-range manifests download each unique complete source through the engine, then select the declared ranges locally. This preserves the existing recovery rules but can consume more bandwidth and storage than selective range fetching.

The plan is frozen when queued; retry does not silently discover a different rendition. Sources, tokens and the normalized plan are protected with Windows DPAPI. History shows the parent media job; child file transfers stay internal. All bulk traffic uses the profile's single engine lock and ordinary queue/limit controls. An admitted media job occupies its queue turn until completion or pause. Discovery is separately bounded (2 MiB manifest, 20 seconds per request, redirect and segment limits); it is outside the transfer speed counter.

## Verification and retention

Downloaded components retain their verified hashes. Assembly checks their bytes again, combines only app-owned local files, and invokes the fixed helper paths. Helpers have embedded build-specific hashes, no network protocols or encoders, a local format allowlist, no arbitrary command options, clean environments and Windows process/memory limits. AAC decoding is enabled for stream information. Every output operation uses `-c copy`. These controls are resource and interface restrictions; they are not a Windows sandbox certification.

The output is checked for selected codecs, dimensions, audio properties, packet counts and recorded color/HDR side data before ordinary origin marking and recoverable no-overwrite publication. Failure keeps source tracks. No silent downgrade, transcoding, skipped-fragment option or file execution is offered. Retained source and assembly files currently require manual cleanup after the user is satisfied; the app does not silently delete them.

The original source, build recipe, LGPL notice and MinGW runtime terms ship in the install's `media` directory. The separate AAC encoder used to generate original tests is a development tool and is not bundled. See [MEDIA-BUILD.md](MEDIA-BUILD.md), [TEST-EVIDENCE.md](TEST-EVIDENCE.md) and [FEATURES.md](FEATURES.md) for the actual evidence and remaining release gates.
