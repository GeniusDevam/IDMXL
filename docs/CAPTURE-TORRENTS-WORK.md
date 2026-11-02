# Capture and torrents — 2 November 2026

Authorized scope: remove the capture receipt window; native ownership, one location
dialog and independent controls after successful browser handoff; integrated
BitTorrent support. Preserve the qualified HTTP engine performance and durability.

Implementation underway on `feature/native-capture-torrents`, based on the
qualified performance branch. No unattended changes to Chrome security settings.

Plan:
1. Native capture with optional origin-scoped, protected browser request context;
   no strong-ETag prerequisite for sequential downloads; cancel Chrome before
   committing, hold its target dialog, erase its cancelled history item.
2. Stop silently representing Chrome-owned transfers as successful captures.
   Browser-only POST/blob downloads stay explicit browser downloads.
3. Bundle libtorrent 2.1.1 as a separate background engine. Support v1/v2/hybrid,
   magnet/local/URL torrent input, files/priorities, trackers/webseeds, peers,
   limits, queueing/seeding, pause/resume/recheck/retry/relocation/removal,
   durable fast-resume and crash checking, torrent creation and OS opening.
4. Add desktop controls and browser torrent handoff, package dependencies and
   notices, run real local peer-transfer/recovery tests and HTTP/extension checks,
   build and update the installed app. Record precisely what passed.

The user's clarification window was the first ten minutes. Routine choices will
be made autonomously; torrent bandwidth defaults are unlimited download and
conservative upload, adjustable by the user. No unrelated downloads are removed.

Completed: implementation, 238 checks, isolated browser/native UI checks, packaged TCP/uTP crash recovery, NSIS build, installation and all 109 installed-file verifications. Chrome Reload remains a user action after the Computer Use URL-policy stop. Final qualification: CAPTURE-TORRENTS.md.
