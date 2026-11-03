import { useEffect, useRef, useState } from "react";

import { invoke } from "@tauri-apps/api/core";

import { open, save } from "@tauri-apps/plugin-dialog";

import { ConfirmAction } from "./controls";

import { getCurrentWebview } from "@tauri-apps/api/webview";

type Torrent = {
  id: string;
  name: string;
  folder: string;
  state: string;
  total_bytes: string | null;
  received_bytes: string;
  download_speed: string;
  upload_speed: string;
  uploaded_bytes: string;
  peers: number;
  seeds: number;
  progress: number;
  error: string | null;
  has_metadata: boolean;
  sequential: boolean;
  seed_ratio: number;
  version: string;
  private: boolean;
  already_added?: boolean;
};

type File = {
  index: number;
  path: string;
  size: string;
  received: string;
  priority: number;
  pad: boolean;
};

type Details = Torrent & {
  info_hash: string;
  files: File[];
  file_count: number;
  file_offset: number;
  priorities: number[];
  trackers: { url: string; tier: number; verified: boolean }[];
  peers_detail: {
    address: string;
    port: number;
    client: string;
    down_speed: number;
    up_speed: number;
  }[];
  magnet: boolean;
};

type Settings = {
  download_limit: number;
  upload_limit: number;
  connections: number;
  per_torrent_connections: number;
  active_downloads: number;
  active_seeds: number;
  seed_ratio: number;
  listen_port: number;
  listen_address: string;
  dht: boolean;
  pex: boolean;
  lsd: boolean;
  port_mapping: boolean;
  utp:boolean;
  encryption: string;
};

type Confirmation = {
  title: string;
  description: string;
  confirmLabel: string;
  run: () => Promise<void>;
};

const request = <T,>(value: Record<string, unknown>) =>
  invoke<T>("torrent_request", { request: value });

const size = (value: string | number | null) =>
  value === null
    ? "Unknown size"
    : Number(value) >= 1073741824
      ? `${(Number(value) / 1073741824).toFixed(2)} GB`
      : `${(Number(value) / 1048576).toFixed(2)} MB`;

const message = (e: unknown) =>
  ({
    torrent_metadata_pending:
      "Still finding torrent information. Check that peers or trackers are available.",
    torrent_files_exist:
      "Files already exist here. Choose another folder or explicitly choose to verify and reuse them.",
    torrent_worker_missing:
      "The torrent component is missing. Reinstall the complete IDMXL package.",
    unsafe_torrent_path: "This torrent contains an unsafe file path.",
    torrent_not_active:
      "This torrent was removed. Add its torrent file or magnet link again.",
    torrent_location_not_confirmed:
      "Choose a folder and confirm the torrent first.",
  })[String(e)] || String(e).replaceAll("_", " ");

export function Torrents({ onJobs }: { onJobs?: (jobs: Torrent[]) => void }) {
  const [jobs, setJobs] = useState<Torrent[]>([]),
    [settings, setSettings] = useState<Settings | null>(null);

  const [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [source, setSource] = useState("");

  const [showAdd, setShowAdd] = useState(false),
    [showSettings, setShowSettings] = useState(false),
    [showCreate, setShowCreate] = useState(false);

  const [selected, setSelected] = useState<string | null>(null),
    [detail, setDetail] = useState<Details | null>(null);

  const [folder, setFolder] = useState(""),
    [priorities, setPriorities] = useState<number[]>([]),
    [reuse, setReuse] = useState(false);

  const [confirmation, setConfirmation] = useState<Confirmation | null>(null),
    [tracker, setTracker] = useState("");

  const [createPath, setCreatePath] = useState(""),
    [createTrackers, setCreateTrackers] = useState(""),
    [createVersion, setCreateVersion] = useState("hybrid"),
    [createPrivate, setCreatePrivate] = useState(false);

  const dialog = useRef<HTMLDialogElement>(null),
    addDialog = useRef<HTMLDialogElement>(null),
    settingsDialog = useRef<HTMLDialogElement>(null),
    createDialog = useRef<HTMLDialogElement>(null);

  const [fileOffset, setFileOffset] = useState(0);

  const shown = useRef(new Set<string>()),
    polling = useRef(false);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type !== "drop") return;

        const paths = event.payload.paths.filter((p) =>
          p.toLowerCase().endsWith(".torrent"),
        );

        if (paths.length)
          void run(async () => {
            for (const source of paths)
              await request({ kind: "inspect", source });
          });
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});

    const paste = (event: ClipboardEvent) => {
      if (
        !event.isTrusted ||
        (event.target instanceof Element &&
          event.target.closest("input,textarea,[contenteditable]"))
      )
        return;

      const sources = (event.clipboardData?.getData("text/plain") || "")
        .split(/\r?\n/)
        .map((s) => s.trim())
        .filter(
          (s) =>
            s.startsWith("magnet:?") ||
            /^https?:\/\/\S+\.torrent(?:\?\S*)?$/i.test(s),
        );

      if (!sources.length) return;
      event.preventDefault();
      event.stopImmediatePropagation();

      void run(async () => {
        for (const source of sources)
          await request({ kind: "inspect", source });
      });
    };

    document.addEventListener("paste", paste, true);

    return () => {
      disposed = true;
      unlisten?.();
      document.removeEventListener("paste", paste, true);
    };
  }, []);

  async function refresh() {
    if (polling.current) return;

    polling.current = true;

    try {
      const snapshot = await request<{ jobs: Torrent[]; settings: Settings }>({
        kind: "list",
      });

      setJobs(snapshot.jobs);
      onJobs?.(snapshot.jobs);

      setSettings((old) => old || snapshot.settings);

      const draft = snapshot.jobs.find(
        (j) => j.state === "selecting" && !shown.current.has(j.id),
      );

      if (draft && !selected) {
        shown.current.add(draft.id);
        choose(draft.id);
      }

      if (selected)
        setDetail(
          await request<Details>({
            kind: "details",
            id: selected,
            offset: fileOffset,
          }),
        );
    } catch (e) {
      setError(message(e));
    } finally {
      polling.current = false;
    }
  }

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 1500);
    return () => window.clearInterval(timer);
  }, [selected, fileOffset]);

  useEffect(() => {
    if (selected) dialog.current?.showModal();
    else dialog.current?.close();
  }, [selected]);

  useEffect(() => {
    if (showAdd) addDialog.current?.showModal();
    else addDialog.current?.close();
  }, [showAdd]);

  useEffect(() => {
    if (showSettings) settingsDialog.current?.showModal();
    else settingsDialog.current?.close();
  }, [showSettings]);

  useEffect(() => {
    if (showCreate) createDialog.current?.showModal();
    else createDialog.current?.close();
  }, [showCreate]);

  async function run(fn: () => Promise<unknown>) {
    setBusy(true);
    setError("");
    try {
      await fn();
      await refresh();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }

  function choose(id: string) {
    setSelected(id);
    setFileOffset(0);
    setDetail(null);
    setPriorities([]);
    setReuse(false);
    setFolder("");
    void invoke<string>("download_location")
      .then(setFolder)
      .catch(() => {});
  }

  async function action(
    job: Torrent,
    action: string,
    extra: Record<string, unknown> = {},
  ) {
    await request({ kind: "control", id: job.id, action, ...extra });
  }

  function confirmAction(targets: Torrent[], actionName: string) {
    setConfirmation({
      title: `${actionName === "delete" ? "Delete files for" : actionName === "remove" ? "Remove" : actionName === "pause" ? "Pause" : "Cancel"} ${targets.length === 1 ? targets[0].name : `all ${targets.length} torrents`}?`,
      description:
        actionName === "delete"
          ? "This permanently deletes this torrent's downloaded and partial files from this PC. Cancel or close this window to keep them."
          : actionName === "remove"
            ? "Remove these torrents from IDMXL. Their files stay on this PC."
            : "Transfers and seeding will stop. Existing files stay on this PC.",
      confirmLabel: actionName === "delete" ? "Confirm deletion" : "Confirm",
      run: async () => {
        for (const job of targets)
          await action(job, actionName, { confirmed: true });
        if (actionName === "remove" || actionName === "delete")
          setSelected(null);
        await refresh();
      },
    });
  }

  async function closeDetails() {
    if (detail?.state === "selecting")
      await run(() => action(detail, "remove"));

    setSelected(null);
  }

  return (
    <section className="torrent-panel" aria-label="Torrents">
      <div className="torrent-heading">
        <div>
          <h2>Torrents</h2>
          <p className="form-note">
            Magnet links and torrent files · verified pieces · background
            transfers
          </p>
        </div>
        <div className="heading-actions">
          <button className="primary" onClick={() => setShowAdd(true)}>
            Add torrent
          </button>
          <button className="secondary" onClick={() => setShowCreate(true)}>
            Create torrent
          </button>
          <button className="secondary" onClick={() => setShowSettings(true)}>
            Torrent settings
          </button>
        </div>
      </div>

      {error && (
        <p role="alert" className="error-message">
          {error}
          <button onClick={() => setError("")}>Dismiss</button>
        </p>
      )}

      {jobs.length > 0 && (
        <>
          <div className="heading-actions">
            <button
              className="secondary"
              onClick={() =>
                confirmAction(
                  jobs.filter(
                    (j) => !["completed", "canceled"].includes(j.state),
                  ),
                  "pause",
                )
              }
            >
              Pause all torrents
            </button>
            <button
              className="secondary"
              onClick={() => confirmAction(jobs, "cancel")}
            >
              Cancel all torrents
            </button>
            <button
              className="secondary danger"
              onClick={() => confirmAction(jobs, "delete")}
            >
              Delete all torrent files
            </button>
          </div>

          <div className="torrent-table">
            <table>
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Progress</th>
                  <th>Speed</th>
                  <th>Peers</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {jobs.map((job) => (
                  <tr key={job.id}>
                    <td>
                      <button
                        className="text-button"
                        onClick={() => choose(job.id)}
                      >
                        {job.name}
                      </button>
                      <small>
                        {job.state} · {job.version}
                        {job.private ? " · private" : ""}
                      </small>
                    </td>

                    <td>
                      <progress value={job.progress} max={1} />
                      <small>
                        {size(job.received_bytes)} / {size(job.total_bytes)}
                      </small>
                    </td>

                    <td>
                      <span>↓ {size(job.download_speed)}/s</span>
                      <small>↑ {size(job.upload_speed)}/s</small>
                    </td>
                    <td>
                      {job.peers} ({job.seeds} seeds)
                    </td>

                    <td>
                      <div className="heading-actions">
                        {[
                          "downloading",
                          "metadata",
                          "checking",
                          "seeding",
                          "queued",
                        ].includes(job.state) ? (
                          <button
                            className="secondary"
                            disabled={busy}
                            onClick={() => void run(() => action(job, "pause"))}
                          >
                            Pause
                          </button>
                        ) : (
                          job.state !== "selecting" && (
                            <button
                              className="secondary"
                              disabled={busy}
                              onClick={() =>
                                void run(() => action(job, "retry"))
                              }
                            >
                              {job.state === "failed"
                                ? "Retry now"
                                : job.state === "completed"
                                  ? "Seed"
                                  : "Resume"}
                            </button>
                          )
                        )}
                        <button
                          className="secondary"
                          onClick={() => choose(job.id)}
                        >
                          {job.state === "selecting"
                            ? "Choose files"
                            : "Details"}
                        </button>
                        <button
                          className="secondary"
                          onClick={() => confirmAction([job], "cancel")}
                        >
                          Cancel
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      <dialog
        ref={addDialog}
        className="workflow-dialog"
        onCancel={(e) => {
          e.preventDefault();
          setShowAdd(false);
        }}
      >
        <div className="dialog-heading">
          <h2>Add torrent</h2>
          <button
            aria-label="Close add torrent"
            className="icon-button"
            onClick={() => setShowAdd(false)}
          >
            ×
          </button>
        </div>

        <label>
          Magnet link, torrent URL or local file
          <input
            value={source}
            onChange={(e) => setSource(e.target.value)}
            placeholder="magnet:?xt=… or https://…/file.torrent"
          />
        </label>

        <button
          className="secondary"
          onClick={() =>
            void open({
              multiple: true,
              filters: [{ name: "Torrent files", extensions: ["torrent"] }],
            }).then((files) => {
              if (files)
                setSource((Array.isArray(files) ? files : [files]).join("\n"));
            })
          }
        >
          Browse torrent files
        </button>

        <div className="form-actions">
          <button className="secondary" onClick={() => setShowAdd(false)}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={busy || !source.trim()}
            onClick={() =>
              void run(async () => {
                const sources = source
                  .split(/\r?\n/)
                  .map((s) => s.trim())
                  .filter(Boolean);
                for (const value of sources)
                  await request({ kind: "inspect", source: value });
                setSource("");
                setShowAdd(false);
              })
            }
          >
            Review files
          </button>
        </div>
        <p className="form-note">
          No file data downloads until you confirm the folder and files.
        </p>
      </dialog>

      <dialog
        ref={dialog}
        className="workflow-dialog torrent-dialog"
        onCancel={(e) => {
          e.preventDefault();
          if (!busy) void closeDetails();
        }}
      >
        <div className="dialog-heading">
          <h2>{detail?.name || "Torrent details"}</h2>
          <button
            className="icon-button"
            aria-label="Close torrent details"
            disabled={busy}
            onClick={() => void closeDetails()}
          >
            ×
          </button>
        </div>

        {detail && (
          <>
            <p>
              {detail.state} · {detail.version} ·{" "}
              {detail.private ? "Private torrent" : "Public torrent"}
            </p>
            <p className="form-note">{detail.info_hash}</p>

            {detail.state === "selecting" ? (
              <>
                <label>
                  Save files in
                  <input
                    value={folder}
                    onChange={(e) => setFolder(e.target.value)}
                  />
                </label>
                <button
                  className="secondary"
                  onClick={() =>
                    void open({
                      directory: true,
                      multiple: false,
                      defaultPath: folder || undefined,
                      title: "Choose torrent download folder",
                    }).then((v) => {
                      if (typeof v === "string") setFolder(v);
                    })
                  }
                >
                  Choose folder
                </button>
                <label className="check-label">
                  <input
                    type="checkbox"
                    checked={reuse}
                    onChange={(e) => setReuse(e.target.checked)}
                  />
                  Verify and reuse existing files in this folder
                </label>
              </>
            ) : (
              <p className="form-note">{detail.folder}</p>
            )}

            {!detail.has_metadata && (
              <p role="status">
                Finding torrent information from peers. You can wait or cancel.
              </p>
            )}

            {detail.files.length > 0 && (
              <>
                <p className="form-note">
                  Files {detail.file_offset + 1}–
                  {detail.file_offset + detail.files.length} of{" "}
                  {detail.file_count}
                </p>
                <div className="heading-actions">
                  <button
                    className="secondary"
                    disabled={fileOffset === 0}
                    onClick={() => setFileOffset(Math.max(0, fileOffset - 500))}
                  >
                    Previous files
                  </button>
                  <button
                    className="secondary"
                    disabled={
                      detail.file_offset + detail.files.length >=
                      detail.file_count
                    }
                    onClick={() =>
                      setFileOffset(detail.file_offset + detail.files.length)
                    }
                  >
                    More files
                  </button>
                </div>
                <div className="torrent-files">
                  <table>
                    <thead>
                      <tr>
                        <th>Download / priority</th>
                        <th>File</th>
                        <th>Size</th>
                      </tr>
                    </thead>
                    <tbody>
                      {detail.files
                        .filter((f) => !f.pad)
                        .map((f) => (
                          <tr key={f.index}>
                            <td>
                              <select
                                aria-label={`Priority for ${f.path}`}
                                value={priorities[f.index] ?? f.priority}
                                onChange={(e) =>
                                  setPriorities((current) => {
                                    const copy = current.length
                                      ? current.slice()
                                      : detail.priorities;
                                    copy[f.index] = Number(e.target.value);
                                    return copy;
                                  })
                                }
                              >
                                <option value={0}>Skip</option>
                                <option value={1}>Low</option>
                                <option value={4}>Normal</option>
                                <option value={7}>High</option>
                              </select>
                            </td>
                            <td>{f.path}</td>
                            <td>{size(f.size)}</td>
                          </tr>
                        ))}
                    </tbody>
                  </table>
                </div>

                {detail.state !== "selecting" && (
                  <button
                    className="secondary"
                    disabled={busy || !priorities.length}
                    onClick={() =>
                      void run(() =>
                        action(detail, "priorities", { priorities }),
                      )
                    }
                  >
                    Apply file selection
                  </button>
                )}
              </>
            )}

            {detail.state === "selecting" ? (
              <div className="form-actions">
                <button
                  className="secondary"
                  disabled={busy}
                  onClick={() => void closeDetails()}
                >
                  Cancel torrent
                </button>
                <button
                  className="primary"
                  disabled={busy || !detail.has_metadata || !folder}
                  onClick={() =>
                    void run(async () => {
                      await request({
                        kind: "confirm",
                        id: detail.id,
                        folder,
                        priorities: priorities.length
                          ? priorities
                          : detail.priorities,
                        reuse_existing: reuse,
                        start: true,
                      });
                      setSelected(null);
                    })
                  }
                >
                  {busy ? "Starting…" : "Start download"}
                </button>
              </div>
            ) : (
              <>
                <div className="heading-actions">
                  <button
                    className="secondary"
                    disabled={busy}
                    onClick={() => void run(() => action(detail, "recheck"))}
                  >
                    Verify files
                  </button>
                  <button
                    className="secondary"
                    disabled={busy}
                    onClick={() => void run(() => action(detail, "reannounce"))}
                  >
                    Find peers now
                  </button>
                  <button
                    className="secondary"
                    disabled={busy}
                    onClick={() =>
                      void run(async () => {
                        const folder = await open({
                          directory: true,
                          multiple: false,
                          title: "Move torrent files to folder",
                        });
                        if (typeof folder === "string")
                          setConfirmation({
                            title: "Move torrent files?",
                            description: `Move ${detail.name} to ${folder}. Existing files will be kept if the move cannot finish.`,
                            confirmLabel: "Move files",
                            run: async () => {
                              await action(detail, "relocate", {
                                folder,
                                confirmed: true,
                              });
                              await refresh();
                            },
                          });
                      })
                    }
                  >
                    Move files…
                  </button>
                  <button
                    className="secondary"
                    onClick={() => confirmAction([detail], "remove")}
                  >
                    Remove from list
                  </button>
                  <button
                    className="secondary danger"
                    onClick={() => confirmAction([detail], "delete")}
                  >
                    Delete files…
                  </button>
                </div>

                <label className="check-label">
                  <input
                    type="checkbox"
                    checked={detail.sequential}
                    onChange={(e) =>
                      void run(() =>
                        action(detail, "sequential", {
                          enabled: e.target.checked,
                        }),
                      )
                    }
                  />
                  Download in order (normal piece selection is usually faster)
                </label>

                <details>
                  <summary>Trackers and peers</summary>
                  <p>
                    {detail.peers} peers · {detail.seeds} seeds · uploaded{" "}
                    {size(detail.uploaded_bytes)}
                  </p>
                  {detail.trackers.map((t, i) => (
                    <p key={i}>
                      {t.url} · {t.verified ? "responded" : "waiting"}
                    </p>
                  ))}
                  <label>
                    Add tracker or HTTP web seed
                    <input
                      value={tracker}
                      onChange={(e) => setTracker(e.target.value)}
                    />
                  </label>
                  <div className="heading-actions">
                    <button
                      className="secondary"
                      disabled={!tracker}
                      onClick={() =>
                        void run(async () => {
                          await action(detail, "tracker", { url: tracker });
                          setTracker("");
                        })
                      }
                    >
                      Add tracker
                    </button>
                    <button
                      className="secondary"
                      disabled={!tracker}
                      onClick={() =>
                        void run(async () => {
                          await action(detail, "webseed", { url: tracker });
                          setTracker("");
                        })
                      }
                    >
                      Add web seed
                    </button>
                  </div>
                  {detail.peers_detail.map((p, i) => (
                    <p key={i}>
                      {p.address}:{p.port} · {p.client} · ↓ {size(p.down_speed)}
                      /s
                    </p>
                  ))}
                </details>

                <div className="heading-actions">
                  <button
                    className="secondary"
                    disabled={!detail.magnet}
                    onClick={() =>
                      void run(async () => {
                        const value = await request<{ magnet: string }>({
                          kind: "control",
                          id: detail.id,
                          action: "magnet",
                        });
                        await navigator.clipboard.writeText(value.magnet);
                      })
                    }
                  >
                    Copy magnet link
                  </button>
                  <button
                    className="secondary"
                    onClick={() =>
                      void run(async () => {
                        const path = await save({
                          defaultPath: `${detail.name}.torrent`,
                          filters: [
                            { name: "Torrent", extensions: ["torrent"] },
                          ],
                        });
                        if (path) await action(detail, "export", { path });
                      })
                    }
                  >
                    Export torrent…
                  </button>
                </div>
              </>
            )}
          </>
        )}
      </dialog>

      <dialog
        ref={settingsDialog}
        className="workflow-dialog"
        onCancel={(e) => {
          e.preventDefault();
          setShowSettings(false);
          setSettings(null);
        }}
      >
        <div className="dialog-heading">
          <h2>Torrent settings</h2>
          <button
            className="icon-button"
            aria-label="Close torrent settings"
            onClick={() => setShowSettings(false)}
          >
            ×
          </button>
        </div>
        {settings && (
          <>
            <p className="form-note">
              Download and upload limits are in bytes per second. Zero means
              unlimited. Seeding shares verified pieces with other peers.
            </p>

            {(
              [
                ["download_limit", "Download speed limit"],
                ["upload_limit", "Upload speed limit"],
                ["connections", "Total peer connections"],
                ["per_torrent_connections", "Connections per torrent"],
                ["active_downloads", "Active downloads"],
                ["active_seeds", "Active seeds"],
                ["seed_ratio", "Stop seeding at ratio (0 = when finished)"],
                ["listen_port", "Incoming port (0 = automatic)"],
              ] as const
            ).map(([key, label]) => (
              <label key={key}>
                {label}
                <input
                  type="number"
                  min="0"
                  step={key === "seed_ratio" ? "0.1" : "1"}
                  value={settings[key]}
                  onChange={(e) =>
                    setSettings({ ...settings, [key]: Number(e.target.value) })
                  }
                />
              </label>
            ))}

            {(
              [
                ["dht", "DHT peer discovery"],
                ["pex", "Peer exchange"],
                ["lsd", "Local peer discovery"],
              ["utp", "uTP transport (TCP is always available)"],
                ["port_mapping", "Automatic router port mapping"],
              ] as const
            ).map(([key, label]) => (
              <label className="check-label" key={key}>
                <input
                  type="checkbox"
                  checked={settings[key]}
                  onChange={(e) =>
                    setSettings({ ...settings, [key]: e.target.checked })
                  }
                />
                {label}
              </label>
            ))}

            <label>
              Bind to IP address (all = available network interfaces)
              <input
                value={settings.listen_address}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    listen_address: e.target.value.trim(),
                  })
                }
              />
            </label>
            <label>
              Peer encryption
              <select
                value={settings.encryption}
                onChange={(e) =>
                  setSettings({ ...settings, encryption: e.target.value })
                }
              >
                <option value="enabled">
                  Prefer encryption, allow compatible peers
                </option>
                <option value="forced">Require encryption</option>
                <option value="disabled">Disabled</option>
              </select>
            </label>

            <div className="form-actions">
              <button
                className="secondary"
                onClick={() => setShowSettings(false)}
              >
                Cancel
              </button>
              <button
                className="primary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await request({ kind: "settings", settings });
                    setShowSettings(false);
                    setSettings(null);
                  })
                }
              >
                Save settings
              </button>
            </div>
          </>
        )}
      </dialog>

      <dialog
        ref={createDialog}
        className="workflow-dialog"
        onCancel={(e) => {
          e.preventDefault();
          setShowCreate(false);
        }}
      >
        <div className="dialog-heading">
          <h2>Create a torrent</h2>
          <button
            className="icon-button"
            aria-label="Close create torrent"
            onClick={() => setShowCreate(false)}
          >
            ×
          </button>
        </div>
        <label>
          File or folder
          <input
            value={createPath}
            onChange={(e) => setCreatePath(e.target.value)}
          />
        </label>
        <div className="heading-actions">
          <button
            className="secondary"
            onClick={() =>
              void open({ multiple: false }).then((p) => {
                if (typeof p === "string") setCreatePath(p);
              })
            }
          >
            Choose file
          </button>
          <button
            className="secondary"
            onClick={() =>
              void open({ directory: true, multiple: false }).then((p) => {
                if (typeof p === "string") setCreatePath(p);
              })
            }
          >
            Choose folder
          </button>
        </div>
        <label>
          Format
          <select
            value={createVersion}
            onChange={(e) => setCreateVersion(e.target.value)}
          >
            <option value="hybrid">Hybrid v1 + v2</option>
            <option value="v1">BitTorrent v1</option>
            <option value="v2">BitTorrent v2</option>
          </select>
        </label>
        <label>
          Trackers (one per line)
          <textarea
            value={createTrackers}
            onChange={(e) => setCreateTrackers(e.target.value)}
          />
        </label>
        <label className="check-label">
          <input
            type="checkbox"
            checked={createPrivate}
            onChange={(e) => setCreatePrivate(e.target.checked)}
          />
          Private torrent
        </label>
        <div className="form-actions">
          <button className="secondary" onClick={() => setShowCreate(false)}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={busy || !createPath}
            onClick={() =>
              void run(async () => {
                const output = await save({
                  defaultPath: "download.torrent",
                  filters: [{ name: "Torrent", extensions: ["torrent"] }],
                });
                if (output) {
                  await request({
                    kind: "create",
                    path: createPath,
                    output,
                    version: createVersion,
                    private: createPrivate,
                    trackers: createTrackers
                      .split(/\r?\n/)
                      .map((t) => t.trim())
                      .filter(Boolean),
                  });
                  setShowCreate(false);
                }
              })
            }
          >
            {busy ? "Hashing files…" : "Create torrent…"}
          </button>
        </div>
      </dialog>

      <ConfirmAction
        request={confirmation}
        onClose={() => setConfirmation(null)}
        onError={(e) => setError(message(e))}
      />
    </section>
  );
}
