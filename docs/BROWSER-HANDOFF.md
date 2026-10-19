# Browser integration

Extension **0.1.5** supports verified native takeover and browser-assisted compatibility. Use the matching updated desktop/native host. The same `extensions/edge` folder works with Edge and Chrome. The extension is currently unpacked; browser-store publication and broader workflow qualification remain open.

## Connect the application

1. Build or install IDMXL. In your browser's extensions page, enable Developer mode and load `extensions/edge` unpacked. Copy its 32-letter extension ID.
2. Create your download folder. From a normal Windows PowerShell session under the same user as the browser, register the host:

```powershell
.\scripts\browser-host.ps1 -Action Install -Browser Chrome -ExtensionId <32-letter-id> -DownloadDirectory C:\Downloads -Executable <installed-folder>\idmxl.exe
```

For Edge, use `-Browser Edge`. For a source checkout, use `target\release\idmxl.exe`. `-Action Prepare` writes setup files without registration and supports an isolated `-StateDirectory`; actual registration uses the current user's `%LOCALAPPDATA%\IDMXL` profile. Uninstall removes registration while preserving history and files.

3. In the popup, choose **Check IDMXL connection**. If the host cannot be found, verify the browser/extension ID and fully exit/reopen the browser after registration.
4. Choose **Enable automatic capture** and approve the browser permission request yourself if shown. This requests downloads, webRequest and HTTP/HTTPS site access. Already-enabled all-sites capture uses compatibility mode after upgrading/reloading; folder-only settings remain limited until this button is used.

Reload an unpacked extension after updating its files. Browser installation, developer-mode changes and security/permission prompts are user actions. IDMXL does not change browser policy or dismiss security warnings. The shared host configuration permits one exact extension origin; both browsers need the same ID when sharing this profile.

## What happens when you download

IDMXL opens or restores its existing interface. The receipt and desktop distinguish these modes:

| Mode | Transfer and controls | Save As behavior |
| --- | --- | --- |
| Native IDMXL download | A compatible public HTTP(S) file larger than 1,000,000 bytes is verified, handed off and run through IDMXL's queue. Native connections, retries, schedules, limits and file hashing apply. | Choose the location in IDMXL's Save As dialog. Cancel or closing it cancels the download. Chrome's filename decision waits for the handoff, and its transfer is canceled before native commit. |
| **Browser transfer** | Chrome/Edge continues the original download using its original browser session. IDMXL shows browser-reported progress and offers pause/resume/cancel and completed-file reveal. | **Choose Save.** Canceling the dialog cancels this actual transfer. Keep the browser open. |

Browser-assisted mode covers small/zero-byte files, unknown-length downloads, browser-generated files and transfers that require cookies, POST or other browser session state, when exposed by the browser downloads API. It never retries them as a separate HTTP request. Manual browser pauses stay paused. Resume depends on the browser's `canResume` result. Browser security decisions require your action in the browser. Canceling a browser transfer follows the browser's partial-file behavior.

Native queue start/pause, connection limits, speed limits, priority and schedules do **not** control browser transfers. Their bytes and completion state are reported by the browser; IDMXL does not compute or claim a SHA-256 hash for them. If the browser stops reporting, IDMXL shows the disconnected condition. Reopening it reconnects tracked items when browser history is still available. Clearing browser history can remove the ability to control/reveal those items.

Incognito downloads are excluded. Browser-blocked requests that never become downloads, media players without an actual download, other applications' downloads and operating-system transfers are outside this integration. No universal interception, session-bound acceleration or website-specific compatibility is claimed.

## Native handoff and recovery

The native path requires an observed public final GET, exact length, strong ETag, uncompressed representation and an independent credential-free range check of that same representation. Redirect/query links and late size/name/header events can qualify. Cookie presence alone does not reject a public file: independent verification must succeed without those cookies. Authorization-bearing, private-network, ambiguous and unverified sources cannot become native takeovers.

The extension journals intent, pauses Chrome, asks for the native save location and prepares a held native job and persists cancellation intent. It cancels Chrome only after preparation succeeds; commit requires confirmed browser cancellation. Failures before commit abort preparation and restore Chrome when possible. Explicitly canceling or closing the location chooser is different: its decision is journaled and Chrome is canceled, including after a lost reply/restart. Lost acknowledgements/restarts reconcile the same durable ID. An ambiguous canceled handoff requires attention rather than an invented successful browser transfer. Fallback waits briefly for late headers and cannot duplicate an already prepared/committed takeover.

A fresh native commit starts IDMXL's queue and opens/restores its interface. Existing schedules, limits and earlier jobs may affect its start. Duplicate acknowledgements preserve a later manual pause; previously paused/canceled/failed native jobs are not automatically resumed. Only fixed companion executables are launched; downloaded applications are never executed.

**Turn capture off** stops new takeovers and browser tracking, preserves existing files/history and leaves browser-granted permissions intact. Browser downloads continue independently; reconnecting controls requires capture/compatibility enabled again. Disabling during native preparation releases Chrome; already committed native jobs remain in IDMXL.

## Progress, controls and privacy

The bridge sends a filename basename, origin only, browser state, byte counts, speed and resume/file-existence flags. It sends no full source/session URL, local filesystem path, cookies, Authorization headers or POST body. A random client ID owns each record; download ID plus start time prevent controls targeting a reused browser ID. Browser records never enter the native scheduler. History is bounded to 1,000 browser records; terminal records may be retired when full without canceling Chrome.

Pause/resume/cancel/reveal and confirmed desktop file-deletion requests are typed commands for the matching original browser item. They are acknowledged, expire after two minutes and are not blindly replayed after a worker restart or lost acknowledgement. Failed/expired commands show a control error. The extension never approves dangerous files, launches files or chooses an arbitrary native path. Short native-message calls and progress polling keep the extension connected while the browser is running; the native host is not held open permanently.

The popup's **Ongoing downloads** list polls the native host for redacted job summaries and offers per-file Pause, Resume, Cancel and Retry now. It covers native jobs as well as browser transfers. Native destination confirmation/default learning and desktop confirmed file/bulk actions are described in [DOWNLOAD-CONTROLS.md](DOWNLOAD-CONTROLS.md).

**Capture details → Refresh capture details** shows worker readiness and bounded latest outcome/connection issues without source URLs or credentials. Browser-transfer receipts do not assert cancellation or native integrity verification. Very fast downloads can finish before IDMXL opens and appear completed.

## Explicit links and media

Right-click a public file link and choose **Queue link in IDMXL**, then confirm its native save location, for explicit native enqueue. This does not cancel another browser transfer; open/start IDMXL or leave its queue running. Matching repeated request IDs are durable/idempotent. Browser credentials are not transferred.

**Video and audio in this tab** inspects media elements after your click. Select a supported public source, exact tracks and output. This is not a universal video-site extractor. Session-bound/blob players, DRM, live/encrypted streams and playlists are outside native media scope. Browser compatibility applies only if the browser actually starts a download. See [MEDIA.md](MEDIA.md).

## Protocol and qualification

The stdio host accepts bounded 256 KiB native-endian length-prefixed UTF-8 JSON from the exact allowed extension origin and rejects unknown fields/actions. Version 1 enqueues explicit links; version 2 prepares/status-checks/commits/aborts native capture; version 3 handles supported media; version 4 synchronizes browser metadata and typed control acknowledgements. Version 5 serves bounded ongoing-job summaries and typed per-job start/pause/cancel/retry requests; it accepts no source URLs, destinations or file-deletion requests. Version 4 accepts no replay URLs, local destinations or credential fields. Browser-origin native HTTP requests retain public-DNS validation/pinning and redirect checks; browser metadata never triggers an engine HTTP request.

Actual Edge explicit handoff/download/hash, Chrome connection and Chrome native auto-open/start have historical qualification. Browser-assisted 0.1.4 has deterministic protocol/coordinator/control fixtures; actual browser qualification is recorded separately in [TEST-EVIDENCE.md](TEST-EVIDENCE.md). Never infer universal website coverage from those fixtures.

Primary references: [Chrome downloads API](https://developer.chrome.com/docs/extensions/reference/api/downloads), [Chrome native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [Edge native messaging](https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/native-messaging), [optional permissions](https://developer.chrome.com/docs/extensions/reference/api/permissions).
