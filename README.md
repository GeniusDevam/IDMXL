# IDMXL

**A Windows download manager for files and supported video/audio streams.** IDMXL combines a desktop interface, a persistent download queue, safe resume checks and optional browser integration. It uses a Rust/libcurl engine with a Tauri/React interface.

**Status: 0.1.0 development preview for Windows 11 x64.** Everyday downloads and a defined media subset are implemented. This is an independent project; it is not affiliated with Internet Download Manager and does not provide full IDM feature parity.

## Features

| Area | What IDMXL supports |
| --- | --- |
| File downloads | HTTP/HTTPS links, a single-request mode for uncertain or one-use links, and up to four connections for safely reusable links. |
| Pause and recovery | Persistent checkpoints, process-crash recovery and validation of saved data before resume. Sources without safe range support restart as a single stream. |
| Connection controls | Automatic or fixed connection counts, saved retry budgets, request/failure counters, per-file and queue speed limits. |
| Queues and schedules | Named queues, priorities, scheduled starts/deadlines and recurring weekday or overnight hours. One file runs at a time. |
| Link import | Multiple links, Ctrl+V and text/link drops, review before adding, file-type folder rules and collision-safe filenames. |
| Expired links | Replace a file URL; retain saved bytes only when the original trusted hash and strict resource-identity checks allow it. |
| Verification | Optional expected SHA-256, no silent overwrites and Windows Internet-zone metadata for completed downloads. |
| Desktop controls | Searchable history, progress/speed/ETA, light/dark themes, reveal in folder, tray support and background downloads after closing the interface. |
| Completion alerts | Optional Windows notifications while the interface is running; disabled by default. |
| Browser integration | An unpacked extension for Edge and Chrome with explicit public-link handoff, optional automatic capture of supported downloads over 1 MB across websites, and clicked media-element discovery. See qualification details below. |
| Video and audio | Direct media, restricted clear HLS VOD and static DASH; exact track selection, audio-only output and verified assembly without re-encoding or upscaling. |
| Local state | SQLite history and Windows user-scoped protection for source URLs. Python is not required to run the built application. |

## Get started

Public prebuilt installers are not published yet. Build from source using the steps below. Existing preview installers are unsigned, and online updates are not configured.

### Requirements

- Windows 11 x64. Local NTFS folders are the qualified storage target.
- Git and Rust/rustup, with Cargo available in your terminal. The repository pins Rust **1.99.0**.
- Microsoft C++ Build Tools with the **Desktop development with C++** workload and a Windows SDK.
- Node.js **22.12 or newer in the 22.x series**, including npm.
- Microsoft Edge WebView2 Runtime for the desktop interface.
- Internet access for the initial dependency and media-tool builds. The media script downloads pinned source/compiler archives and verifies their hashes.

### Build and launch

Open PowerShell in the folder where you want the source code, then run:

```powershell
git clone https://github.com/GeniusDevam/IDMXL.git
cd IDMXL

# Build the controlled media tools, then the application.
.\scripts\media-build.ps1 -BuildDirectory .\.local\media-build
.\scripts\build.ps1 -Release -MediaBuildDirectory .\.local\media-build

# Open the desktop app.
.\scripts\start.ps1 -Release
```

The first build compiles native dependencies and can take a while. These commands use a normal installed Rust toolchain; no development workspace outside the clone is required. If your organization's PowerShell policy blocks local scripts, follow its approved procedure.

After building, you can also double-click **Start-IDMXL.cmd** in the repository folder. It opens the release build when present, otherwise the debug build.

To create a per-user Windows installer:

```powershell
.\scripts\build.ps1 -Bundle -MediaBuildDirectory .\.local\media-build
```

The installer is written under `apps/desktop/src-tauri/target/release/bundle/nsis/`. Review the [release checklist](docs/RELEASE-CHECKLIST.md) and [dependency record](docs/DEPENDENCIES.md) before distributing a binary.

### Download your first file

1. Choose **New download**, paste a direct HTTP/HTTPS file link and choose its destination.
2. Leave the reusable-link option off for unknown or one-use links. Enable it only when the source permits probing and repeated requests.
3. Add the download, then start it or start its queue. If the queue is already running, an eligible new job can start immediately.
4. Open a file's details to adjust connections, retries, speed, queue, priority or schedule. Use **Import links** for multiple files or **Download media** for supported media sources.

Closing the window hides the interface to the tray while the worker continues. **About this preview → Exit interface — keep downloads running** exits the interface without stopping its worker. Reopen IDMXL to see progress or completed history. Downloaded applications are never launched automatically.

History and settings are stored in `%LOCALAPPDATA%\IDMXL`. Downloaded files stay in the destinations you choose. Cancelling a job preserves its partial files; IDMXL does not silently delete them.

## Connect Edge or Chrome

Load `extensions/edge` as an unpacked extension in an Edge or Chrome development profile, copy its extension ID and register the native host for that browser. The folder works with both browsers; Chrome registration uses `-Browser Chrome`. The [browser setup guide](docs/BROWSER-HANDOFF.md) gives the exact commands and permission details.

Right-click a public file link and choose **Queue link in IDMXL**. The application must be built or installed and registered with that exact extension ID. Run registration from a normal Windows PowerShell session. Restart the browser fully after setup if its connection check cannot find the host.

For automatic capture, open the extension, choose **Enable automatic capture** and approve its browser permission request. It considers downloads larger than **1,000,000 bytes** across HTTP/HTTPS websites, including verified final links reached through redirects. Start IDMXL's queue to run captured jobs automatically. Login-dependent, browser-only, dangerous or unverified downloads continue in the browser; this does not intercept every possible browser download. Existing folder-only settings remain limited until you enable the new all-sites mode. Reload an already-loaded development extension after updating its files.

Extension 0.1.2 confirms the capture worker is running before showing **On**. Successful takeovers open a confirmation window. A real Chrome takeover of the official Node.js Windows ZIP is confirmed; its completed file/hash check is still pending. If Chrome leaves its Save As dialog open after the capture receipt, cancel that dialog and start the queued file in IDMXL. If a file stays in Chrome, expand **Capture details** and refresh it to see the reason. Public downloads can qualify even when the browser sends cookies, but the engine independently verifies them without those cookies; login-dependent downloads still stay in Chrome.

Real Edge connection, explicit context-menu handoff and a completed download with an independently matching hash have passed on the development PC. Chrome's actual popup connection check also passed on 17 October 2026; Chrome file handoff remains to be tested. Automatic capture and the browser media popup have automated coverage but still need broader real-browser qualification. The extension is not published in a browser store.

## Supported scope and limitations

- Media support is limited to the formats and manifest rules in [MEDIA.md](docs/MEDIA.md). DRM, encrypted/live streams, browser-session downloads, blob-only players, arbitrary video-site extraction and playlists are unsupported. No universal codec/HDR or 8K compatibility is claimed.
- Schedules require a logged-in user and a running queue worker. They do not wake a sleeping PC. Strict quotas and metered-network policies are not implemented.
- FTP, website crawling, synchronization, proxy/authentication adapters and ZIP preview are outside this release.
- Code signing and production update hosting are not configured. Use manual updates; the included updater needs a release authority before deployment. See [update setup](docs/UPDATES.md).
- Clean-machine installation/upgrade, wider browser/workflow coverage, accessibility and binary distribution review remain open. Connection tuning does not guarantee a speed advantage over other downloaders.

The last complete local engine/media validation recorded **115 passing automated checks** (34 Rust, 53 Python and 28 Node), plus a separate public-file native-protocol download/hash check. The extension 0.1.2 follow-up passed **46 focused extension checks**; the native engine is unchanged. The optional >4 GiB case was not rerun; its earlier evidence remains historical. [Windows CI for the earlier 0.1.1 checkpoint](https://github.com/GeniusDevam/IDMXL/actions/runs/39021873201) passed. See the [test evidence](docs/TEST-EVIDENCE.md) for versions, limits and remaining qualification. These results do not cover every Windows environment or supported-looking website.

## CLI and development

The engine can run independently of the desktop. See the [CLI guide](docs/CLI.md) for queueing, pause/resume, policies, protected input and expired-link replacement.

For the complete fixture suite, install Python 3.11+ and the test dependencies, then run:

```powershell
python -m pip install -r tests/requirements.txt
.\scripts\test.ps1 -Faults -Protocols -Media -MediaBuildDirectory .\.local\media-build
```

Tests use generated data and loopback servers. The separate optional large-file test needs additional disk space; see the test evidence. Test runtimes are not shipped with IDMXL.

| Location | Contents |
| --- | --- |
| `crates/engine` | Transfers, resource validation, checkpoints, queues and publication recovery. |
| `crates/cli` | Command-line interface and browser native host. |
| `apps/desktop` | Tauri/React desktop application. |
| `extensions/edge` | Development browser extension. |
| `scripts` | Build, launch, validation and release helpers. |
| `tests` | Deterministic transfer, recovery, protocol and media fixtures. |
| `docs` | [Feature matrix](docs/FEATURES.md), [architecture](docs/ARCHITECTURE.md), [workflows](docs/WORKFLOWS.md), [plan](docs/PLAN.md) and release evidence. |

## Licensing

A product license for IDMXL's own source has not been selected. Public repository visibility does not designate an open-source license. Third-party components retain their own licenses and notices; see [DEPENDENCIES.md](docs/DEPENDENCIES.md). No rights to Internet Download Manager's code or branding are implied.
