# Edge and Chrome integration

Extension **0.2.0** uses native IDMXL ownership for captured downloads. The same `extensions/edge` folder works in both browsers. Use the matching 0.2.0 app and native host. Browser-store publication remains separate from unpacked setup.

## Connect the application

1. Build or install IDMXL. On the browser's extensions page, load `extensions/edge` unpacked and copy its 32-letter extension ID.
2. Create the download folder and register from a normal Windows PowerShell session under the same user as the browser:

```powershell
.\scripts\browser-host.ps1 -Action Install -Browser Chrome -ExtensionId <32-letter-id> -DownloadDirectory C:\Downloads -Executable <installed-folder>\idmxl.exe
```

For Edge use `-Browser Edge`. A source build uses `target\release\idmxl.exe`. `Prepare` supports an isolated `StateDirectory`; `Install` uses the current user's `%LOCALAPPDATA%\IDMXL`. Uninstalling registration preserves files/history.

3. Use **Check IDMXL connection** in the extension popup. If the host is not found after registration, fully exit and reopen the browser.
4. Choose **Enable automatic capture** and approve the download/site permissions if requested. Existing all-sites grants do not require new permissions for this update. Reload an already loaded unpacked extension after updating.

Browser installation, Developer mode and browser security approvals remain user actions. IDMXL does not change browser policy. The shared native-host configuration allows one exact extension origin; both browsers need the same ID when sharing it.

## Successful native capture

Ordinary HTTP(S) downloads over **1,000,000 bytes** are prepared by the native engine. IDMXL's destination chooser asks where to save; Cancel or closing it cancels the download. A folder used four times becomes the suggestion. Chrome's filename callback remains pending during this decision.

After successful preparation the extension cancels Chrome's original transfer, confirms that cancellation, durably commits native ownership and erases the cancelled browser history entry. IDMXL opens and starts its queue automatically. There is no extra capture receipt window. Only the IDMXL destination applies; the app and extension Pause/Resume/Cancel operate on the native transfer.

Observed GET request cookies, Authorization, Referer and selected compatibility headers can be forwarded to the native host, protected with Windows DPAPI on disk and applied only to their original origin. They are never persisted in extension storage or forwarded to another origin. Strong browser ETags are checked. A missing or weak ETag can use a single stream; verified range support can retain accelerated, durable resume.

If preparation fails before Chrome cancellation, Chrome is released and the popup reports that it remains a browser download. It is not silently represented as a successful capture. POST bodies, blob/data URLs, incognito downloads, browser security approvals and requests that cannot be verified for replay remain in Chrome. Those cases can still show Chrome's own chooser and controls. Existing historical Browser transfer records describe their previous owner; the automatic capture worker no longer creates that fallback mode.

## Torrents, explicit links and media

Torrent-file downloads are captured at any size and open IDMXL's file/folder review. Payload transfer starts after confirmation. Magnet links work by real click, context menu, app paste and Windows handler registration. See [the torrent guide](TORRENTS.md).

Explicit **Queue link in IDMXL** requests enqueue a separate native transfer after confirming its location; they do not cancel an unrelated download that was already started. Automatic capture performs the ownership handoff above.

**Video and audio in this tab** inspects media elements after a click. Public direct files, supported clear HLS VOD and static DASH are supported. This is not a universal video-site extractor; see [MEDIA.md](MEDIA.md).

## Protocol and qualification

The stdio host accepts bounded 256 KiB native-endian framed UTF-8 JSON from one allowed extension origin. Versions 1-5 retain legacy explicit/capture/media, historical browser-metadata and ongoing-control protocols. Version 6 adds session-aware native prepare/status/commit/abort. Version 7 reviews torrent or magnet sources. Native public-address DNS/redirect checks remain enforced. Browser messages do not select executable paths or command lines. Torrent controls in ongoing summaries use typed torrent identifiers.

The popup polls ongoing native jobs and torrents. **Capture details** reports bounded readiness/outcome messages without URLs, cookies or Authorization values. The new tests and limits are in [0.2.0 qualification](CAPTURE-TORRENTS.md).

References: [downloads API](https://developer.chrome.com/docs/extensions/reference/api/downloads), [native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [optional permissions](https://developer.chrome.com/docs/extensions/reference/api/permissions).
