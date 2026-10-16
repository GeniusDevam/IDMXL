# Download lists, folder rules and recurring hours

Implemented 16 October 2026. These workflows manage direct HTTP/HTTPS file links. They do not discover page links, extract video streams or transfer browser login sessions.

## Import links

Choose **Import links**, paste one file link per line and select an existing download folder. One link is also accepted. Choose a named queue, then add the links. The limit is 1,000 links and 1 MiB of source-link text. A running queue can start newly added jobs immediately when its hours and other policies allow.

Each entry reports its result. Duplicate normalized links inside the submitted list are skipped; fragments do not create a second download. Query strings remain part of source identity and are protected in the job store. Accepted lines are removed from the input, leaving rejected lines available for correction. A second deliberate import is a new operation; it is not a durable browser-handoff transaction.

Leave **Allow probing and parallel requests** off for one-use or uncertain links. Enabling it permits the engine's existing conditional-range policy, with the same identity checks as individually added jobs. Imports never inherit browser cookies or authentication.

## Automatic names and folders

Filenames come from the last URL path component, percent-decoded and sanitized for Windows. The importer does not use query tokens as filenames. Missing names become `download.bin`; Windows device names are made safe. Filename selection reserves a destination in the same SQLite transaction that creates the queued job. Existing files and prior reservations receive numbered alternatives such as `report (2).pdf`. Publication still refuses replacement if another file appears later.

Enable **Organize into folders by file type** to use the editable Folder rules. Defaults cover Documents, Archives, Images, Music, Videos and Programs. Unmatched filenames go into Other. Extensions are compared case-insensitively, and each extension can belong to one rule. Rules run in displayed order; the first matching suffix wins, including compound suffixes such as tar.gz. Rules use a single subfolder name, not an arbitrary path. Existing category reparse points are refused. The broader filesystem/reparse-race qualification remains open.

**Save folder preferences** persists the destination and rules without adding work. Adding links also saves those preferences. These rules apply to Import links and the `add-links` command. They do not move existing downloads, override an explicit Save as path or infer a type from response headers.

## Recurring queue hours

Open a file's details, save its queue name if needed, and use **Recurring hours for <queue>**. The setting applies to every automatically queued job with that queue name.

- Select weekdays and start/end times in the computer's local time zone.
- Start is inclusive and end is exclusive. A 22:00–06:00 Monday window continues into Tuesday morning.
- Equal start/end means the entire selected calendar day.
- Keep the queue worker running while signed in. There is no wake-from-sleep or Windows service.
- Completed files remain completed; recurring hours do not repeatedly download the same URL.
- A closed queue does not block other eligible named queues.
- When hours close, segmented downloads yield after a new verified checkpoint. Sequential/single-use transfers finish their original request, avoiding an automatic replay caused only by the window ending.
- Explicit per-file Start/Resume runs immediately, outside automatic queue hours. Per-file Pause remains explicit and persistent.

Disabling recurring hours removes that queue's recurring restriction. One-time start/deadline, server cooldown and queue-running policies still apply to automatic admission. Sleep, clock changes and the full daylight-saving/time-zone transition matrix are not yet qualified.

## CLI inputs

`queue-window <queue>` reads the current setting. Add `--set` and pipe a JSON window or `null` to change/remove it. Day bits run Monday=1 through Sunday=64; Monday–Friday is 31.

```powershell
'{"days":31,"start_minute":1320,"end_minute":360}' | .\target\debug\idmxl.exe queue-window Night --set
'null' | .\target\debug\idmxl.exe queue-window Night --set
```

`folders` returns saved settings and default rules. `folders --set` reads a settings object. `add-links` reads this shape from stdin:

```json
{
  "links": ["https://example.org/report.pdf", "https://example.org/archive.zip"],
  "folders": {
    "directory": "C:\\Downloads",
    "organize": true,
    "rules": [
      {"folder": "Documents", "extensions": ["pdf", "docx"]},
      {"folder": "Archives", "extensions": ["zip", "7z"]}
    ]
  },
  "queue_name": "Night",
  "replayable": false
}
```

Both workflows use the same engine store as the desktop. The existing explicit-destination `batch` command remains available. Signed URLs belong on stdin, rather than process arguments or logs.

## Upgrade and evidence

Schema v4 adds recurring queue settings and a destination lookup index. Upgrading an existing profile requires idle engine and queue-worker locks, and retains a version-labelled pre-upgrade snapshot. Generated files and old history were preserved during the native v3-to-v4 smoke.

The recorded run passed 19 Rust checks, 42 Python fixtures and 2 extension checks. Native QA imported two generated files, skipped one duplicate, reopened recurring settings, observed waiting outside hours and completed both files after removing that restriction. Both independent hashes and ZoneId=3 checks passed. See [test evidence](TEST-EVIDENCE.md) and [machine-readable results](evidence/workflows.json).
