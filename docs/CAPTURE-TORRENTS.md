# Native capture and torrents — 0.2.0

Local qualification completed on 2 November 2026. The 0.2.0 installer updated the existing installation; all **109 installed runtime files** matched the staged package, with resolved paths checked. See [machine-readable results](evidence/capture-torrents.json).

## Browser workflow

The old successful-capture receipt popup has been removed from the running
capture path. Native preparation obtains the destination before starting. The
extension keeps Chrome's filename decision pending, cancels Chrome's transfer,
durably commits native ownership, removes the cancelled browser history entry
and activates IDMXL. The filename callback is never released after successful
takeover or cancellation of the IDMXL destination chooser.

The old automatic browser-assisted fallback is no longer attached. It was the
reason Chrome's location, Pause and Cancel controlled some entries shown in
IDMXL. Downloads are now reported as captured only when the native engine owns
them. Origin-scoped GET request cookies and permitted headers can be protected
and reused by native requests. Cross-origin redirects do not receive those
credentials. Strong browser ETags are checked during preparation; missing/weak
ETags can use sequential downloads instead of forcing a browser-owned transfer.

Unexportable POST/blob/data transfers, incognito downloads, unsafe downloads and
unverifiable requests remain in Chrome. No universal browser takeover is
claimed. Chrome's API does not export every browser transfer into a standalone
engine. Normal files retain the >1,000,000-byte threshold; torrent files can be
handed off at any size.

## Torrent workflow

See [the torrent guide](TORRENTS.md). The worker is a fixed bundled executable
using native libtorrent 2.1.1. An authenticated framed loopback interface exposes
typed download operations. Website input cannot choose executable paths or
commands. Torrent history/resume data use a separate protected local database.
The HTTP history schema is upgraded from 7 to 8, with a version-7 backup, to
store protected native request context.

## Evidence collected so far

- Real isolated Chromium/native-host handoffs for strong-ETag, no-ETag,
  cookie-dependent and unknown-size generated downloads. Independent SHA-256
  checks passed; captured Chrome download entries were absent, no receipt window
  appeared, and native extension Pause/Resume/Cancel worked.
- Cancelling the destination choice prevented native download creation.
- Local peer transfers cover v1 file selection, v2 transfers, hybrid/magnet
  metadata, pause/restart, independent hashes, corrupted-piece repair,
  relocation/export and scoped deletion. Packaged TCP and uTP transfers survived forced process termination and passed final independent hashes. Two additional uTP recovery confirmations passed.
- The previous speed engine and its measured results are retained; this change
  does not claim a new internet speed benchmark or universal IDM superiority.

References: [Chrome downloads API](https://developer.chrome.com/docs/extensions/reference/api/downloads),
[native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging),
[IDM's destination workflow](https://www.internetdownloadmanager.com/support/using_idm/using_idm.html),
[libtorrent disk-flush contract](https://www.libtorrent.org/reference-Torrent_Handle.html#flush_cache).

## Final validation and delivery

**238 unique automated checks passed:** 72 Rust, 76 HTTP/TLS/media integration,
12 torrent and 78 JavaScript checks. Root/native lint, frontend compilation and
NSIS packaging passed. Five isolated-browser scenarios exercised native handoff,
independent controls and location cancellation. The separate historical >4 GiB
resource test was not repeated. No new public-internet speed comparison is claimed.

Qualification exposed stale checkpoint flags, queued-save races and premature
100% progress. The worker now clears metadata-only flags for confirmed jobs,
saves the newest state after pending checkpoints, preserves user intent and
waits for disk flush. The local uTP fixture was also corrected to create its UDP
seed transport before listening and allow multiple loopback clients; production
peer rules were not relaxed to satisfy that fixture.

The application was updated in `LocalAppData/Programs/IDMXL` with the NSIS
installer (exit 0). The previous runtime was backed up outside the installation.
The update did not delete or reset user profile data. Normal-session history was
not inspected after installation; version-7 preservation/backup is covered by an
automated migration test.

Computer Use inspected the native torrent Add and Settings dialogs and their
cancellation behavior in an isolated profile. It then stopped while trying to
navigate Chrome's extension manager because it could not confidently identify
the current browser URL. No further desktop input was issued. **The existing
Chrome extension still needs Reload at chrome://extensions.** On-disk files are
0.2.0; its currently loaded version is not claimed verified. The signed public
release and browser-store distribution remain separate work.
