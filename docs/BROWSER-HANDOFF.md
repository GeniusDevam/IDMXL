# Browser integration (development)

Load `extensions/edge` unpacked in an Edge development profile and register its exact 32-letter ID. Extension installation, developer-mode changes and browser permission prompts are user setup steps. The installed application includes extension source and this guide. Store publication and Chrome qualification are separate work.

## Connect the application

Build the preview or use an installed copy. Create the download folder, then run:

```powershell
.\scripts\browser-host.ps1 -Action Install -ExtensionId <32-letter-id> -DownloadDirectory C:\Downloads -Executable <installed-folder>\idmxl.exe
```

For a source checkout, pass `target\release\idmxl.exe`. `-Action Prepare` writes configuration without registering it and permits an isolated `-StateDirectory`. Actual registration uses the current user's `%LOCALAPPDATA%\IDMXL` profile. `-Browser Chrome` selects a separately qualified browser registration. Uninstall removes only that registration and preserves downloads/history. Choose **Check IDMXL connection** in the popup to diagnose a missing registration, rejected ID or startup failure. Restart the browser after setup if necessary.

## Explicit file links

Right-click a public file link and choose **Queue link in IDMXL**. A transaction records a durable handoff ID before acknowledgement. Repeating an ID with the same request returns the same job; a different request is rejected. Open IDMXL to start it, or leave its automatic queue running. This action does not cancel another browser download. Signed public links can be protected locally, but browser cookies and authentication are not transferred.

## Optional public-file capture

Capture is off by default. Enter an exact HTTPS folder URL ending in `/` under **Optional public-file capture**, then choose **Enable for this folder**. The user must grant the browser's optional downloads, webRequest and site permissions. The rule applies to that origin and directory prefix; **Turn capture off** removes the rules. Permission grants remain browser-managed.

Only files of at least 1 MiB with an observed, unambiguous public GET, strong ETag and exact length are eligible. Private/incognito, cookie/authorization-bearing, tokenized, redirected, ranged, dangerous, compressed, unobserved or ambiguous transfers stay in the browser. Headers and URLs are not written to the capture recovery log. The engine independently checks the exact representation and public DNS addresses.

The extension persists intent, pauses and verifies the browser transfer, prepares a held native job, then persists cancellation intent. It cancels the browser transfer only after preparation succeeds. Native commit requires confirmed browser cancellation. Before that point, failures resume the browser transfer and abort preparation. Lost acknowledgements and worker restarts query the same durable ID; ambiguous cases report attention. Expired preparation can be renewed only after an exact fresh identity check. Held jobs cannot be started through ordinary queue/CLI entry points.

## Video and audio

**Video and audio in this tab** inspects media elements only after a click. Choose a public source, inspect original tracks, select a rendition/audio and queue it. This uses activeTab/scripting; there is no automatic page script or universal site extractor. Blob players, browser-bound sessions and protected/live streams are unsupported. See [MEDIA.md](MEDIA.md).

## Protocol and boundary

The same `idmxl.exe` is the stdio native host. It requires an exact allowed extension origin, accepts bounded 256 KiB native-endian length-prefixed UTF-8 JSON, rejects unknown fields/actions and chooses safe names inside the configured folder. Version 1 enqueues explicit links; version 2 prepares/status-checks/commits/aborts capture; version 3 inspects and queues selected media. Pages cannot choose arbitrary local paths or command options. Sources/plans are DPAPI protected.

Browser-origin requests validate all DNS answers on each request and redirect and pin transport to public addresses. Loopback/private/link-local/mapped/multicast/special-use addresses and environment proxies are rejected. Desktop/CLI input can intentionally address an intranet. No cookies, POST replay or enterprise sessions are transferred. Standard tests cover hostile frames, durable duplicate handling and capture failure/restart sequences; real-browser evidence and remaining qualification are in [TEST-EVIDENCE.md](TEST-EVIDENCE.md).

Primary references: [Edge native messaging](https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/native-messaging), [Chrome native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [optional permissions](https://developer.chrome.com/docs/extensions/reference/api/permissions).
