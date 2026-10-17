# Browser integration (development)

Load `extensions/edge` unpacked in an Edge or Chrome development profile and register its exact 32-letter ID. This folder works with both browsers. Extension installation, developer-mode changes and browser permission prompts are user setup steps. The installed application includes extension source and this guide. Store publication and broader browser workflow qualification remain open.

## Connect the application

Build the preview or use an installed copy. Create the download folder, then run:

```powershell
.\scripts\browser-host.ps1 -Action Install -ExtensionId <32-letter-id> -DownloadDirectory C:\Downloads -Executable <installed-folder>\idmxl.exe
```

For a source checkout, pass `target\release\idmxl.exe`. `-Action Prepare` writes configuration without registering it and permits an isolated `-StateDirectory`. Actual registration uses the current user's `%LOCALAPPDATA%\IDMXL` profile. Add `-Browser Chrome` to select Chrome; the default is Edge. Run installation from a normal Windows PowerShell session under the same user as the browser. Uninstall removes only that registration and preserves downloads/history. Choose **Check IDMXL connection** in the popup to diagnose a missing registration, rejected ID or startup failure. Restart the browser after setup if necessary.

The shared host configuration permits one exact extension ID. Registering a different ID replaces that permitted origin; using both browsers with this profile requires the same extension ID in each browser.

## Explicit file links

Right-click a public file link and choose **Queue link in IDMXL**. A transaction records a durable handoff ID before acknowledgement. Repeating an ID with the same request returns the same job; a different request is rejected. Open IDMXL to start it, or leave its automatic queue running. This action does not cancel another browser download. Signed public links can be protected locally, but browser cookies and authentication are not transferred.

## Automatic capture over 1 MB

Open the extension and choose **Enable automatic capture**, then grant the browser's optional downloads, webRequest and HTTP/HTTPS site permissions. This enables capture across websites without entering individual folders. **Turn capture off** stops future takeovers and preserves existing jobs and browser-managed permission grants. Capture remains off until enabled. Previously saved folder-only rules retain their scope until the new button is used. Reload a loaded development extension after replacing its files.

The threshold is strictly larger than **1,000,000 bytes** (1 MB); files at or below it stay in the browser. Downloads whose size or filename arrives later are reconsidered when Chrome reports the change. HTTP and HTTPS final URLs, redirects and reusable query/signed URLs are supported when the final observed GET has a strong ETag and exact length, and the engine independently confirms that same representation using a public, credential-free range request. Final links that redirect again during that probe are rejected. The full final URL, including its query, remains bound to the job; URLs are protected in native state and are not persisted in the extension's recovery journal.

This is not universal interception. Private/incognito, authorization-bearing, POST-only, blob/data, ranged, dangerous, compressed, unobserved, ambiguous, unknown-length and weakly identified downloads remain in Chrome. Downloads initiated by another extension are also left alone. If the browser finishes first, it keeps the completed download. Cookie presence alone does not reject a public file: the engine must independently verify the same representation without browser cookies, and cookie-dependent sources fail that preparation and resume in Chrome. Cookie and other credential values are never copied to the engine or recovery journal. These restrictions preserve a working browser transfer when a safe handoff cannot be established.

The extension persists intent, pauses and verifies the browser transfer, prepares a held native job, then persists cancellation intent. It cancels the browser transfer only after preparation succeeds. Native commit requires confirmed browser cancellation. Before that point, failures resume the browser transfer and abort preparation. Lost acknowledgements and worker restarts query the same durable ID; ambiguous cases report attention. Expired preparation can be renewed only after an exact fresh identity check. Held jobs cannot be started through ordinary queue/CLI entry points.

With extension 0.1.3 and the matching native app, a successful automatic capture opens or restores IDMXL and starts the download queue. The prepared job stays held until confirmed browser cancellation and durable commit. Only a fresh commit enables the queue; duplicate acknowledgements do not override a later manual pause. Manually paused, canceled and failed jobs remain excluded. Saved queue hours, deadlines, priorities and speed limits still apply, so an eligible job starts when its turn arrives. Other eligible queued jobs can also run once the queue starts. Turning capture off during preparation releases the browser transfer; already committed jobs remain in IDMXL. Download event listeners register synchronously when the extension worker starts, and later size/name changes use the same serialized, duplicate-safe handoff path.

Extension 0.1.3 also checks the live worker's listener registration before showing **On**, and refreshes registration after permission changes. It reconsiders downloads when matching response headers arrive later, handles the earlier filename event without blocking Chrome's filename callback, preserves verified navigation responses detached with `ERR_ABORTED`, strips non-network URL fragments, and compares numeric lengths consistently. A successful commit opens an IDMXL confirmation window; a window failure does not undo the handoff. **Capture details → Refresh capture details** shows worker readiness and the latest bounded reason when a transfer stays in Chrome. Neither source URLs nor credential values appear in these diagnostic records.

One real Chrome automatic takeover and its confirmation window were confirmed with the official Node.js Windows ZIP. If Chrome leaves a Save As dialog open behind the capture receipt, cancel that dialog and view progress in IDMXL. This handoff result does not establish completion of that file or compatibility with every website; see [test evidence](TEST-EVIDENCE.md).

## Video and audio

**Video and audio in this tab** inspects media elements only after a click. Choose a public source, inspect original tracks, select a rendition/audio and queue it. This uses activeTab/scripting; there is no automatic page script or universal site extractor. Blob players, browser-bound sessions and protected/live streams are unsupported. See [MEDIA.md](MEDIA.md).

## Protocol and boundary

The same `idmxl.exe` is the stdio native host. It requires an exact allowed extension origin, accepts bounded 256 KiB native-endian length-prefixed UTF-8 JSON, rejects unknown fields/actions and chooses safe names inside the configured folder. Version 1 enqueues explicit links; version 2 prepares/status-checks/commits/aborts capture; version 3 inspects and queues selected media. Pages cannot choose arbitrary local paths or command options. Sources/plans are DPAPI protected.

Browser-origin requests validate all DNS answers on each request and redirect and pin transport to public addresses. Loopback/private/link-local/mapped/multicast/special-use addresses and environment proxies are rejected. Desktop/CLI input can intentionally address an intranet. No cookies, POST replay or enterprise sessions are transferred. Standard tests cover hostile frames, durable duplicate handling and capture failure/restart sequences; real-browser evidence and remaining qualification are in [TEST-EVIDENCE.md](TEST-EVIDENCE.md).

Primary references: [Edge native messaging](https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/native-messaging), [Chrome native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [optional permissions](https://developer.chrome.com/docs/extensions/reference/api/permissions).

## Qualified setup sequence on 16 October 2026

With the installed engine registered, the earlier host-not-found status cleared after fully exiting Edge through **Settings and more → Close Microsoft Edge**, reopening it and having the user dismiss its developer-mode notice. Closing only a window had not resolved the earlier failure. The exact lookup-failure cause was not isolated; no browser security setting was changed. Use the popup's **Check IDMXL connection** to confirm readiness before testing a handoff.

An actual right-click on a controlled local page's public HTTPS source-archive link created one durable request and one queued job. The popup acknowledged it. Running that native job completed a 12,040,788-byte file whose independent SHA-256 matched the packaged original source and whose Internet ZoneId was 3. This qualifies the explicit-link path on this development profile, not automatic capture, every browser or extension-store distribution. See `evidence/edge-native-success.json`.

## Chrome connection on 17 October 2026

The user confirmed **IDMXL is connected and ready.** in Chrome after repairing registration from their normal Windows session. The repair preserved the existing host configuration, manifest and download history. A framed native-host status check also received the expected response. Earlier diagnostic logs reported a missing manifest; the precise difference from the earlier registration was not isolated. Temporary diagnostic logging was confirmed off afterward. No browser policies or security settings were changed.

This verifies the real Chrome connection check only. Chrome context-menu file handoff, optional capture and browser media workflows still need actual browser testing. See `evidence/chrome-native-connection.json`.


## Chrome capture enabled on 17 October 2026

The user supplied a popup screenshot displaying **On: downloads over 1 MB across websites**. The greyed **Enable automatic capture** button is expected while that mode is active; **Turn capture off** remains available. This verifies activation, with no need to repeat the permission step. Start IDMXL's queue to run captured jobs as they arrive. The screenshot's queued-link message is not an automatic-takeover receipt; an actual automatic browser-download/completion test remains open.

## Automatic desktop startup

The installed native host launches only its fixed companion engine and desktop executables, bound to the same profile. A worker readiness acknowledgement confirms the scheduler lock is owned before startup is reported. Windows native-message pipes are excluded from child inheritance. A profile-specific interface lease prevents duplicate windows; later launches request show, restore and focus of the existing window. Capture receipts distinguish confirmed worker startup from an older app or a launch failure, without undoing committed ownership. The downloaded file is never executed.
