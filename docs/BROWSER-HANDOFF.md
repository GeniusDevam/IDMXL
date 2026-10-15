# Explicit browser handoff (development)

The original Manifest V3 extension is in `extensions/edge`. It adds **Queue link in IDMXL** to the link context menu. It has context-menu, native-messaging and local-status storage permissions, with no page/content scripts, cookie access, host permissions, interception, or automatic browser cancellation. Only HTTP(S) GET file links without embedded username/password are accepted. A signed public URL can be handed off; an authenticated browser session is not transferred.

The native host is the same `idmxl.exe`. Chromium passes its extension origin as the first argument. The host requires an exact configured origin. Version 1 messages use a native-endian unsigned 32-bit length followed by UTF-8 JSON, capped at 256 KiB. The only request is:

```json
{"version":1,"kind":"enqueue","request_id":"0f417bf1-a960-462a-a1af-62d427ee655d","url":"https://example.org/file.zip","filename":"file.zip"}
```

Unknown fields, versions, actions, oversized frames, truncated input and unapproved origins are rejected. The host chooses a UUID-prefixed safe filename inside the configured existing download folder; pages cannot choose an arbitrary local path. URLs remain DPAPI protected. A committed transaction creates the job and handoff ID before acceptance is acknowledged. Repeating an ID with the same source returns the same job; a different source is rejected. Jobs start with replay disabled. Nothing automatically cancels an existing browser transfer.

Browser-origin jobs have a separate public-address policy. Every request and redirect validates all DNS answers and pins libcurl to that answer set. Loopback, private, link-local, mapped, multicast and selected special-use ranges are rejected. Proxies are disabled. A direct user-entered desktop/CLI URL can intentionally address an intranet server. This policy does not provide browser cookies, enterprise authentication or POST replay.

## Local setup

Build the engine first. Load `extensions/edge` unpacked in an appropriate Edge development profile, and copy the exact extension ID shown by Edge. Developer-mode/profile changes and extension installation are deliberate user setup steps. Store review and real-browser qualification remain separate gates.

Prepare and inspect configuration without registry changes:

```powershell
.\scripts\browser-host.ps1 -Action Prepare -ExtensionId <32-letter-id> -DownloadDirectory C:\Downloads
```

To register the inspected host for the current user:

```powershell
.\scripts\browser-host.ps1 -Action Install -ExtensionId <32-letter-id> -DownloadDirectory C:\Downloads
```

For an installed release, supply `-Executable <installed-folder>\idmxl.exe`. Use `-Browser Chrome` only for separately tested Chrome setup. `Prepare` accepts a custom isolated `-StateDirectory`; actual Chromium registration uses `%LOCALAPPDATA%\IDMXL\browser-host.json`. Uninstall removes only the selected registry entry and preserves profile/downloads.

The stdio round trip, durable duplicate handling and hostile protocol inputs have deterministic CLI fixtures. Extension behavior has a Node test using a mocked browser API. These checks do not establish a completed Edge UI installation, context-menu click-through or extension-store acceptance.

Primary platform references: [native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging), [context menus](https://developer.chrome.com/docs/extensions/reference/api/contextMenus).
