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

This is not universal interception. Private/incognito, cookie/authorization-bearing, POST-only, blob/data, ranged, dangerous, compressed, unobserved, ambiguous, unknown-length and weakly identified downloads remain in Chrome. Downloads initiated by another extension are also left alone. If the browser finishes first, it keeps the completed download. Credential header values and URLs are not written to the capture recovery log. Browser-only authentication is never copied to IDMXL. These restrictions preserve a working browser transfer when a safe handoff cannot be established.

The extension persists intent, pauses and verifies the browser transfer, prepares a held native job, then persists cancellation intent. It cancels the browser transfer only after preparation succeeds. Native commit requires confirmed browser cancellation. Before that point, failures resume the browser transfer and abort preparation. Lost acknowledgements and worker restarts query the same durable ID; ambiguous cases report attention. Expired preparation can be renewed only after an exact fresh identity check. Held jobs cannot be started through ordinary queue/CLI entry points.

Start IDMXL's queue once to run captured jobs as they arrive. When the queue is stopped, captures are queued for you to start in the desktop app. Turning capture off during preparation releases the browser transfer; already committed jobs remain in IDMXL. Download event listeners register synchronously when the extension worker starts, and later size/name changes use the same serialized, duplicate-safe handoff path.

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
