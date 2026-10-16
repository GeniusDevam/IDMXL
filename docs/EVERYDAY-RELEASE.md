# IDMXL everyday downloads and media

This release targets Windows 11 x64 and local NTFS folders. Advanced FTP, website crawling, synchronization, proxy/authentication adapters and ZIP preview remain outside this release. The private preview is unsigned; online update authority is unset and updates are installed manually.

## Download a file

Choose **New download**, paste the HTTP/HTTPS file link, choose a destination and add it. Start the job or start its queue. Closing the window keeps transfers running. **About this preview → Exit interface — keep downloads running** exits the interface; the download worker continues. Reopen IDMXL to see completed history. Downloaded installers are never run automatically.

Leave the reusable-link option off for unknown or one-use links. This uses one streaming request without a preliminary probe. Enable it only for sources that allow repeated requests: IDMXL checks strong validators and ranges before resuming saved blocks. Sources without safe ranges use a single stream and may restart after interruption. One file runs at a time; resumable files can use up to four connections.

New reusable jobs automatically tune their connection count up to four. Select a job to change its maximum, automatic tuning, retries, speed limit, queue, priority and schedule. Limits use fixed connection admission. Existing jobs retain their saved settings. Adaptation does not guarantee faster downloads on every server or connection.

## Import and organize

Use **Import links**, paste multiple file links with Ctrl+V onto the main window, or drop browser links onto it. Review the list and folder rules before adding. IDMXL does not monitor the clipboard or start transfers on paste/drop. Import accepts up to 1,000 links and 1 MB of text. Existing files are preserved and numbered filenames are chosen for collisions.

Queue hours support weekdays and overnight windows while the worker is running and the user is logged in. They do not wake a sleeping PC. A paused individual job stays paused until explicitly queued or resumed.

## Recover an expired link

Pause other transfers and select a paused/failed/queued ordinary file. Paste the renewed URL under **Replace an expired file link**. With no saved bytes, IDMXL can use the new source on the next run. Reusing saved bytes requires the expected SHA-256 supplied when the job was created, the same HTTPS/HTTP origin and path, and matching strong ETag and exact length from a ranged probe. The expected digest is checked again before publication. A changed or unproven source is rejected without replacing the old source or discarding checkpoints. Browser captures and media require a new job.

Failed block requests and the retry budget are saved before another attempt. Restarting a worker does not replenish a recorded failed-request budget. Explicitly retrying a failed job grants a new budget while retaining historical request/failure counts. The counters describe transfer-body requests, excluding discovery/probe requests and any history predating this version.

## Media

Use **Download media** for direct accessible media or the supported clear HLS VOD/static DASH subset. Choose the exact available main/audio tracks and a compatible output container. The included tools assemble original tracks without encoding or upscaling, and the app verifies the output before marking it complete. Source tracks are retained. See [MEDIA.md](MEDIA.md) for precise manifest and container restrictions.

DRM, encrypted streams, live streams, browser sessions, blob-only players, arbitrary website extraction and playlists are unsupported. The original 8K fixture proves one high-resolution path; it is not a claim of universal codec/HDR compatibility.

## Completion and browser integration

Optional Windows completion notifications are off by default and available in **About this preview**. They require the interface to remain running, are best effort under Windows notification settings, and use fixed text without filenames or source URLs. Completion history is authoritative even when a notification is suppressed.

The development extension and native host are included. Protocol and extension fixtures pass, but the real Edge connection on the development PC still reports **native host not found**. Browser integration is not yet qualified for consumer release. Paste a file link directly into IDMXL while that issue is investigated. See [BROWSER-HANDOFF.md](BROWSER-HANDOFF.md).

## Status and evidence

This is a usable private release candidate for the supported paths, not full IDM feature parity or a qualified public release. Current evidence and unresolved qualification are in [TEST-EVIDENCE.md](TEST-EVIDENCE.md), [FEATURES.md](FEATURES.md) and [RELEASE-CHECKLIST.md](RELEASE-CHECKLIST.md). Code signing, clean-machine/browser qualification, broader codec/HDR coverage and distribution notices remain release work.
