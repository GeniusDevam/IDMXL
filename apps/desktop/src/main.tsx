import React, { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import "./styles.css";
import { ImportLinks, QueueHours } from "./workflows";
import { displayPath } from "./paths";
import { MediaDialog, MediaDetails } from "./media";
import { Updates } from "./updates";
import { importableLinks } from "./links";
import { DownloadManagement, FileControls } from "./controls";

type Job = {
  id: string;
  source: string;
  destination: string;
  name: string;
  state: string;
  generation: number;
  total_bytes: string | null;
  durable_bytes: string;
  received_bytes: string;
  speed_bytes_per_second: string;
  eta_seconds: string | null;
  policy: Policy;
  retry_not_before: string | null;
  mode: string;
  replayable: boolean;
  sha256: string | null;
  error: string | null;
  file_available: boolean;
  queue_window_open: boolean;
  transfer_requests: string;
  failed_requests: string;
};
type Policy = {
  queue_name: string;
  priority: number;
  not_before: number;
  deadline: number | null;
  connections: number;
  adaptive: boolean;
  retries: number;
  speed_limit: number;
};
type Filter = "all" | "active" | "completed";
const activeStates = new Set(["downloading", "pausing", "verifying"]);
const native = isTauri();

function bytes(value: string | bigint | null) {
  if (value === null) return "Unknown size";
  const n = typeof value === "bigint" ? value : BigInt(value);
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let divisor = 1n,
    index = 0;
  while (n / divisor >= 1024n && index < units.length - 1) {
    divisor *= 1024n;
    index++;
  }
  const tenths = (n * 10n) / divisor;
  return `${tenths / 10n}${index ? `.${tenths % 10n}` : ""} ${units[index]}`;
}

function percent(job: Job) {
  if (!job.total_bytes || BigInt(job.total_bytes) === 0n)
    return job.state === "completed" ? 100 : 0;
  return Math.min(
    100,
    Number((BigInt(job.received_bytes) * 100n) / BigInt(job.total_bytes)),
  );
}

function Icon({ name, size = 20 }: { name: string; size?: number }) {
  const paths: Record<string, React.ReactNode> = {
    arrow: (
      <>
        <path d="M12 3v12m-5-5 5 5 5-5" />
        <path d="M4 16v4h16v-4" />
      </>
    ),
    grid: (
      <>
        <rect x="3" y="3" width="7" height="7" rx="1.5" />
        <rect x="14" y="3" width="7" height="7" rx="1.5" />
        <rect x="3" y="14" width="7" height="7" rx="1.5" />
        <rect x="14" y="14" width="7" height="7" rx="1.5" />
      </>
    ),
    activity: <path d="M3 12h4l3-8 4 16 3-8h4" />,
    check: <path d="m5 12 4 4L19 6" />,
    plus: <path d="M12 5v14M5 12h14" />,
    search: (
      <>
        <circle cx="10.5" cy="10.5" r="6.5" />
        <path d="m16 16 5 5" />
      </>
    ),
    file: (
      <>
        <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9Z" />
        <path d="M14 3v6h6M8 14h8M8 17h5" />
      </>
    ),
    pause: (
      <>
        <path d="M8 5v14M16 5v14" />
      </>
    ),
    play: <path d="m8 4 12 8-12 8Z" />,
    close: <path d="m6 6 12 12M18 6 6 18" />,
    shield: (
      <>
        <path d="m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6Z" />
        <path d="m8 12 3 3 5-6" />
      </>
    ),
    folder: <path d="M3 7a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v9H3Z" />,
    info: (
      <>
        <circle cx="12" cy="12" r="9" />
        <path d="M12 11v6M12 7h.01" />
      </>
    ),
  };
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name] || paths.file}
    </svg>
  );
}

function App() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [error, setError] = useState("");
  const [form, setForm] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [importText, setImportText] = useState("");
  const [renewedUrl, setRenewedUrl] = useState("");
  const [notifyCompleted, setNotifyCompleted] = useState(false);
  const [showMedia, setShowMedia] = useState(false);
  const [url, setUrl] = useState("");
  const [destination, setDestination] = useState("");
  const [digest, setDigest] = useState("");
  const [replayable, setReplayable] = useState(false);
  const [working, setWorking] = useState("");
  const [selected, setSelected] = useState<Job | null>(null);
  const [showAbout, setShowAbout] = useState(false);
  const [caps, setCaps] = useState<Record<string, unknown> | null>(null);
  const [queueRunning, setQueueRunning] = useState(false);
  const [queueLimit, setQueueLimit] = useState("0");
  const queueSettingsLoaded = useRef(false);
  const [policyDraft, setPolicyDraft] = useState<Policy | null>(null);
  const [theme, setTheme] = useState(
    localStorage.getItem("idmxl-theme") || "system",
  );
  const modalRef = useRef<HTMLDialogElement>(null);
  const oldFocus = useRef<HTMLElement | null>(null);

  async function refresh() {
    if (!native) return;
    try {
      setJobs(await invoke<Job[]>("list_jobs"));
      const status = await invoke<{ running: boolean; speed_limit: string }>(
        "queue_status",
      );
      setQueueRunning(status.running);
      if (!queueSettingsLoaded.current) {
        setQueueLimit(status.speed_limit);
        queueSettingsLoaded.current = true;
      }
    } catch {
      setError(
        "The download history could not be read. Check that the app can access its data folder.",
      );
    }
  }
  useEffect(() => {
    void refresh();
    if (native)
      void invoke<Record<string, unknown>>("capabilities")
        .then(setCaps)
        .catch(() => {});
    if (native)
      void invoke<{ notify: boolean }>("completion_settings")
        .then((settings) => setNotifyCompleted(settings.notify))
        .catch(() => {});
    const timer = window.setInterval(() => void refresh(), 750);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    if (!native) return;
    function review(text: string) {
      const links = importableLinks(text);
      if (!links) return false;
      setImportText(links);
      setShowImport(true);
      return true;
    }
    function paste(event: ClipboardEvent) {
      const target = event.target as HTMLElement | null;
      if (
        target?.closest("input, textarea, select, [contenteditable='true']") ||
        document.querySelector("dialog[open]")
      )
        return;
      if (review(event.clipboardData?.getData("text/plain") || ""))
        event.preventDefault();
    }
    function drag(event: DragEvent) {
      if (
        event.dataTransfer?.types.some((type) =>
          ["text/uri-list", "text/plain"].includes(type),
        )
      )
        event.preventDefault();
    }
    function drop(event: DragEvent) {
      event.preventDefault();
      if (document.querySelector("dialog[open]")) return;
      review(
        event.dataTransfer?.getData("text/uri-list") ||
          event.dataTransfer?.getData("text/plain") ||
          "",
      );
    }
    window.addEventListener("paste", paste);
    window.addEventListener("dragover", drag);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("paste", paste);
      window.removeEventListener("dragover", drag);
      window.removeEventListener("drop", drop);
    };
  }, []);
  useEffect(() => {
    if (form) {
      oldFocus.current = document.activeElement as HTMLElement;
      modalRef.current?.showModal();
    } else {
      modalRef.current?.close();
      oldFocus.current?.focus();
    }
  }, [form]);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("idmxl-theme", theme);
  }, [theme]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key.toLowerCase() === "n") {
        event.preventDefault();
        if (native && !document.querySelector("dialog[open]")) setForm(true);
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  useEffect(() => {
    if (selected) setSelected(jobs.find((j) => j.id === selected.id) ?? null);
  }, [jobs]);
  useEffect(() => {
    setPolicyDraft(selected?.policy ?? null);
    setRenewedUrl("");
  }, [selected?.id]);

  const active = jobs.filter((j) => activeStates.has(j.state));
  const nativeActive = active.filter((j) => j.mode !== "browser");
  const completed = jobs.filter((j) => j.state === "completed");
  const queued = jobs.filter((j) =>
    ["queued", "paused", "failed"].includes(j.state),
  );
  const completedBytes = completed.reduce(
    (sum, j) => sum + BigInt(j.durable_bytes),
    0n,
  );
  const visible = jobs.filter(
    (j) =>
      (filter === "all" ||
        (filter === "active"
          ? activeStates.has(j.state)
          : j.state === "completed")) &&
      `${j.name} ${j.source}`.toLowerCase().includes(search.toLowerCase()),
  );

  async function action(
    job: Job,
    kind: "start" | "retry" | "pause" | "cancel" | "queue",
  ) {
    setWorking(job.id);
    setError("");
    try {
      await invoke(
        kind === "retry"
          ? "retry_job"
          : kind === "start"
            ? "start_job"
            : "control_job",
        kind === "start" || kind === "retry"
          ? { id: job.id }
          : {
              id: job.id,
              action:
                kind === "pause"
                  ? "paused"
                  : kind === "queue"
                    ? "queued"
                    : "canceled",
            },
      );
      await refresh();
    } catch (e) {
      setError(friendly(e));
    } finally {
      setWorking("");
    }
  }
  async function chooseDestination() {
    let name = "download.bin";
    try {
      name = decodeURIComponent(new URL(url).pathname.split("/").pop() || name);
    } catch {
      /* User may still be typing. */
    }
    const folder = displayPath(await invoke<string>("download_location"));
    const chosen = await save({
      title: "Save download as",
      defaultPath: `${folder}\\${name}`,
    });
    if (chosen) setDestination(chosen);
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setWorking("new");
    setError("");
    try {
      await invoke<Job>("add_job", {
        url,
        destination,
        replayable,
        expectedSha256: digest || null,
      });
      setForm(false);
      setUrl("");
      setDestination("");
      setDigest("");
      setReplayable(false);
      await refresh();
    } catch (e) {
      setError(friendly(e));
    } finally {
      setWorking("");
    }
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <a
          className="brand"
          href="#"
          onClick={(e) => {
            e.preventDefault();
            setFilter("all");
          }}
          aria-label="IDMXL, all downloads"
        >
          <span className="brand-mark">
            <Icon name="arrow" size={23} />
          </span>
          <strong>
            IDM<span>XL</span>
          </strong>
        </a>
        <div className="workspace-label">YOUR WORKSPACE</div>
        <nav aria-label="Download views">
          {(
            [
              ["all", "grid", "All downloads", jobs.length],
              ["active", "activity", "In progress", active.length],
              ["completed", "check", "Completed", completed.length],
            ] as const
          ).map(([value, icon, title, count]) => (
            <button
              key={value}
              className={`nav-item ${filter === value ? "current" : ""}`}
              onClick={() => setFilter(value)}
              aria-current={filter === value ? "page" : undefined}
            >
              <Icon name={icon} />
              <span>{title}</span>
              <small>{count}</small>
            </button>
          ))}
        </nav>
        <div className="sidebar-note">
          <span className="status-dot" />
          Local by design
          <p>
            Your files travel directly
            <br />
            from the source to your PC.
          </p>
        </div>
        <button
          className="about-button"
          onClick={() => setShowAbout(!showAbout)}
        >
          <Icon name="info" size={17} />
          About this preview<span>0.1</span>
        </button>
      </aside>

      <main>
        <header className="topbar">
          <div className="breadcrumb">
            Workspace <span>/</span> Downloads
          </div>
          <span className="preview-label">EARLY PREVIEW</span>
        </header>
        <div className="content">
          <section className="page-heading">
            <div>
              <div className="eyebrow">A LITTLE MORE CONTROL.</div>
              <h1>Your downloads.</h1>
              <p>Pick up where you left off.</p>
            </div>
            <div className="heading-actions">
              <button
                className="secondary"
                disabled={!native}
                onClick={() => setShowMedia(true)}
              >
                Download media
              </button>
              <button
                className="secondary"
                disabled={!native}
                onClick={() => {
                  setImportText("");
                  setShowImport(true);
                }}
              >
                Import links
              </button>
              <button
                className="primary"
                disabled={!native}
                onClick={() => setForm(true)}
              >
                <Icon name="plus" />
                New download
              </button>
            </div>
          </section>
          {!native && (
            <div className="notice">
              <Icon name="info" />
              <div>
                <strong>Desktop preview</strong>
                <p>
                  Open the Windows app to add and manage downloads. This browser
                  preview shows the interface.
                </p>
              </div>
            </div>
          )}
          {error && (
            <div className="error-message" role="alert">
              <span>{error}</span>
              <button onClick={() => setError("")} aria-label="Dismiss error">
                <Icon name="close" size={17} />
              </button>
            </div>
          )}
          {native && (
            <DownloadManagement
              jobs={jobs}
              onChanged={() => void refresh()}
              formatError={friendly}
            />
          )}
          {showAbout && (
            <section className="about-panel">
              <strong>A dependable foundation, in progress.</strong>
              <p>
                This preview supports HTTP and HTTPS file downloads, verified
                checkpoints, pause/resume and SHA-256 checks. One file runs at a
                time, with named queues and scheduled starts. Browser handoff
                needs the separate extension setup. Media downloads preserve
                selected original tracks. Online updates need the release
                owner's signing and hosting setup.
              </p>
              <small>
                {caps
                  ? `libcurl ${caps.libcurl} · ${caps.tls} · ${caps.credential_storage}`
                  : "No account required. Your download history stays on this PC."}
              </small>
              {native && <Updates formatError={friendly} />}
              {native && (
                <>
                  <label className="check-label">
                    <input
                      type="checkbox"
                      checked={notifyCompleted}
                      onChange={async (event) => {
                        try {
                          const settings = await invoke<{ notify: boolean }>(
                            "completion_settings",
                            { enabled: event.target.checked },
                          );
                          setNotifyCompleted(settings.notify);
                        } catch (error) {
                          setError(friendly(error));
                        }
                      }}
                    />{" "}
                    Notify me when downloads finish (while IDMXL is open)
                  </label>
                  <p className="form-note">
                    Paste links here with Ctrl+V, or drop links onto this
                    window, to review them before adding.
                  </p>
                  <button
                    className="secondary"
                    onClick={() =>
                      void invoke("quit_interface").catch((error) =>
                        setError(friendly(error)),
                      )
                    }
                  >
                    Exit interface — keep downloads running
                  </button>
                </>
              )}
            </section>
          )}
          <section className="stats" aria-label="Download summary">
            {[
              [
                "activity",
                "In progress",
                active.length.toString(),
                "Working on it",
              ],
              [
                "check",
                "Completed",
                completed.length.toString(),
                "Ready when you are",
              ],
              [
                "folder",
                "Downloaded",
                bytes(completedBytes),
                "Saved to your PC",
              ],
              [
                "pause",
                "Waiting",
                queued.length.toString(),
                "Start when you choose",
              ],
            ].map(([icon, label, value, note]) => (
              <div className="stat" key={label}>
                <div className="stat-label">
                  <span>{label}</span>
                  <Icon name={icon} size={18} />
                </div>
                <div className="stat-value">{value}</div>
                <small>{note}</small>
              </div>
            ))}
          </section>
          <section className="download-panel" aria-labelledby="downloads-title">
            <div className="queue-toolbar">
              <button
                className="secondary"
                disabled={!native || Boolean(working)}
                onClick={async () => {
                  try {
                    await invoke("queue_control", {
                      running: !queueRunning,
                      speedLimit: queueRunning ? null : queueLimit,
                    });
                    await refresh();
                  } catch (e) {
                    setError(friendly(e));
                  }
                }}
              >
                {queueRunning ? "Pause IDMXL queue" : "Start IDMXL queue"}
              </button>
              <label>
                IDMXL queue limit (bytes/s){" "}
                <input
                  type="number"
                  min="0"
                  max="10000000000"
                  value={queueLimit}
                  onChange={(e) => setQueueLimit(e.target.value)}
                />
              </label>
              <label>
                Theme{" "}
                <select
                  value={theme}
                  onChange={(e) => setTheme(e.target.value)}
                >
                  <option value="system">System</option>
                  <option value="light">Light</option>
                  <option value="dark">Dark</option>
                </select>
              </label>
              <small>
                {queueRunning
                  ? "Queue runs in the background"
                  : "Queue stopped"}
              </small>
            </div>
            {jobs.some((job) => job.mode === "browser") && (
              <p className="form-note">
                Browser transfers use Chrome or Edge. Use each file's controls;
                the IDMXL queue settings apply to its own engine.
              </p>
            )}
            <div className="panel-toolbar">
              <h2 id="downloads-title">
                {filter === "all"
                  ? "All downloads"
                  : filter === "active"
                    ? "In progress"
                    : "Completed"}
                <span>{visible.length}</span>
              </h2>
              <div className="search">
                <Icon name="search" size={17} />
                <input
                  aria-label="Search downloads"
                  placeholder="Find a download…"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
              </div>
            </div>
            {visible.length ? (
              <div className="job-list">
                {visible.map((job) => (
                  <article className="job" key={job.id}>
                    <div
                      className={`file-icon ${job.state === "completed" ? "finished" : ""}`}
                    >
                      <Icon
                        name={job.state === "completed" ? "check" : "file"}
                        size={25}
                      />
                    </div>
                    <div className="job-content">
                      <button
                        className="job-name"
                        onClick={() =>
                          setSelected(selected?.id === job.id ? null : job)
                        }
                      >
                        {job.name}
                      </button>
                      <p className="job-source">{job.source}</p>
                      {job.mode === "browser" && (
                        <span className="local-badge">Browser transfer</span>
                      )}
                      <div
                        className="progress"
                        role="progressbar"
                        aria-label={`${job.name} downloaded`}
                        aria-valuenow={percent(job)}
                        aria-valuemin={0}
                        aria-valuemax={100}
                      >
                        <span style={{ width: `${percent(job)}%` }} />
                      </div>
                      <div className="job-meta">
                        <span>
                          {bytes(job.received_bytes)} / {bytes(job.total_bytes)}
                          {job.state === "downloading" &&
                            ` · ${bytes(job.speed_bytes_per_second)}/s${job.eta_seconds ? ` · ~${job.eta_seconds}s left` : ""}`}
                        </span>
                        <span className={`job-state ${job.state}`}>
                          {job.state === "completed" && !job.file_available
                            ? "File unavailable"
                            : job.state === "queued" && !job.queue_window_open
                              ? "Waiting for queue hours"
                              : job.state.replaceAll("_", " ")}
                        </span>
                      </div>
                      {job.error && (
                        <p className="job-error">{friendly(job.error)}</p>
                      )}
                      {job.retry_not_before && (
                        <p className="job-error">
                          Source retry available after{" "}
                          {new Date(
                            Number(job.retry_not_before),
                          ).toLocaleString()}
                        </p>
                      )}
                    </div>
                    <div className="job-actions">
                      {job.mode !== "browser" &&
                        ["paused", "failed"].includes(job.state) && (
                          <button
                            className="icon-button"
                            title="Add to automatic queue"
                            aria-label={`Queue ${job.name}`}
                            onClick={() => void action(job, "queue")}
                          >
                            <Icon name="plus" />
                          </button>
                        )}
                      {job.file_available && (
                        <button
                          className="icon-button"
                          title="Show in folder"
                          aria-label={`Show ${job.name} in folder`}
                          onClick={() =>
                            void invoke("reveal_job", { id: job.id }).catch(
                              (e) => setError(friendly(e)),
                            )
                          }
                        >
                          <Icon name="folder" />
                        </button>
                      )}
                      {activeStates.has(job.state) ? (
                        <button
                          className="icon-button"
                          title="Pause download"
                          aria-label={`Pause ${job.name}`}
                          disabled={
                            working === job.id || job.state === "pausing"
                          }
                          onClick={() => void action(job, "pause")}
                        >
                          <Icon name="pause" />
                        </button>
                      ) : (
                        !["completed", "canceled", "attention"].includes(
                          job.state,
                        ) && (
                          <button
                            className={
                              job.state === "failed" && job.mode !== "browser"
                                ? "secondary retry-button"
                                : "icon-button"
                            }
                            title={
                              job.mode !== "browser" && nativeActive.length
                                ? "Pause the current download before starting another"
                                : job.state === "failed" &&
                                    job.mode !== "browser"
                                  ? "Retry now"
                                  : "Start or resume"
                            }
                            aria-label={`${job.state === "failed" && job.mode !== "browser" ? "Retry now" : "Start"} ${job.name}`}
                            disabled={
                              Boolean(working) ||
                              (job.mode === "browser"
                                ? !job.replayable
                                : nativeActive.length > 0)
                            }
                            onClick={() =>
                              void action(
                                job,
                                job.state === "failed" && job.mode !== "browser"
                                  ? "retry"
                                  : "start",
                              )
                            }
                          >
                            {job.state === "failed" &&
                            job.mode !== "browser" ? (
                              "Retry now"
                            ) : (
                              <Icon name="play" />
                            )}
                          </button>
                        )
                      )}
                      {!["completed", "canceled", "publishing"].includes(
                        job.state,
                      ) && (
                        <button
                          className="icon-button subtle"
                          title={
                            job.mode === "browser"
                              ? "Cancel browser download"
                              : "Cancel, keeping partial files"
                          }
                          aria-label={`Cancel ${job.name}`}
                          disabled={working === job.id}
                          onClick={() => void action(job, "cancel")}
                        >
                          <Icon name="close" size={18} />
                        </button>
                      )}
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="empty">
                <div className="empty-art">
                  <span />
                  <Icon
                    name={filter === "completed" ? "check" : "arrow"}
                    size={35}
                  />
                  <span />
                </div>
                <h3>
                  {search
                    ? "No matching downloads"
                    : filter === "completed"
                      ? "A home for your finished files."
                      : filter === "active"
                        ? "Nothing running right now."
                        : "Your next download starts here."}
                </h3>
                <p>
                  {search
                    ? "Try a different filename or source."
                    : "Add a file link, choose where to save it, and take it from there."}
                </p>
                {!search && filter === "all" && (
                  <button
                    className="secondary"
                    disabled={!native}
                    onClick={() => setForm(true)}
                  >
                    <Icon name="plus" size={17} />
                    Add your first download
                  </button>
                )}
              </div>
            )}
            <div className="panel-footer">
              <Icon name="shield" size={16} />
              <span>Verified checkpoints. A safer way to resume.</span>
              <span className="local-badge">ON YOUR DEVICE</span>
            </div>
          </section>
          {selected && (
            <section className="details">
              <FileControls
                key={selected.id}
                job={selected}
                onChanged={() => void refresh()}
                formatError={friendly}
              />
              {selected.mode === "browser" && (
                <p className="form-note">
                  Browser transfer: Chrome or Edge handles this file using its
                  existing browser session. Keep that browser open. IDMXL shows
                  progress and sends pause, resume, cancel and show-in-folder
                  requests; native queue limits and schedules do not apply.
                </p>
              )}
              {policyDraft && selected.mode !== "browser" && (
                <form
                  className="policy-form"
                  onSubmit={async (e) => {
                    e.preventDefault();
                    try {
                      await invoke("set_policy", {
                        id: selected.id,
                        policy: policyDraft,
                      });
                      await refresh();
                    } catch (error) {
                      setError(friendly(error));
                    }
                  }}
                >
                  <h3>Queue and schedule</h3>
                  <label>
                    Queue name
                    <input
                      value={policyDraft.queue_name}
                      maxLength={64}
                      onChange={(e) =>
                        setPolicyDraft({
                          ...policyDraft,
                          queue_name: e.target.value,
                        })
                      }
                    />
                  </label>
                  <label>
                    Priority (-1000 to 1000)
                    <input
                      type="number"
                      min="-1000"
                      max="1000"
                      value={policyDraft.priority}
                      onChange={(e) =>
                        setPolicyDraft({
                          ...policyDraft,
                          priority: Number(e.target.value),
                        })
                      }
                    />
                  </label>
                  <label>
                    Start after (local time)
                    <input
                      type="datetime-local"
                      value={
                        policyDraft.not_before
                          ? new Date(
                              policyDraft.not_before -
                                new Date(
                                  policyDraft.not_before,
                                ).getTimezoneOffset() *
                                  60000,
                            )
                              .toISOString()
                              .slice(0, 16)
                          : ""
                      }
                      onChange={(e) =>
                        setPolicyDraft({
                          ...policyDraft,
                          not_before: e.target.value
                            ? new Date(e.target.value).getTime()
                            : 0,
                        })
                      }
                    />
                  </label>
                  <label>
                    Maximum connections (1–8)
                    <input
                      type="number"
                      min="1"
                      max="8"
                      value={policyDraft.connections}
                      onChange={(e) =>
                        setPolicyDraft({
                          ...policyDraft,
                          connections: Number(e.target.value),
                        })
                      }
                    />
                  </label>
                  <label className="check-label">
                    <input
                      type="checkbox"
                      checked={policyDraft.adaptive}
                      onChange={(event) =>
                        setPolicyDraft({
                          ...policyDraft,
                          adaptive: event.target.checked,
                        })
                      }
                    />
                    Automatically tune connections
                  </label>
                  <p className="form-note">
                    Automatic tuning uses up to the selected maximum for
                    resumable downloads. Speed limits use a fixed connection
                    count.
                  </p>
                  <label>
                    Retries per block
                    <input
                      type="number"
                      min="0"
                      max="5"
                      value={policyDraft.retries}
                      onChange={(event) =>
                        setPolicyDraft({
                          ...policyDraft,
                          retries: Number(event.target.value),
                        })
                      }
                    />
                  </label>
                  <label>
                    Job limit (bytes/s; 0 unlimited)
                    <input
                      type="number"
                      min="0"
                      value={policyDraft.speed_limit}
                      onChange={(e) =>
                        setPolicyDraft({
                          ...policyDraft,
                          speed_limit: Number(e.target.value),
                        })
                      }
                    />
                  </label>
                  <p>
                    {bytes(selected.durable_bytes)} saved in verified
                    checkpoints. Scheduled jobs run while the queue worker is
                    running; missed start times run when next eligible.
                  </p>
                  <button className="secondary" type="submit">
                    Save schedule
                  </button>
                </form>
              )}
              {selected.mode !== "browser" && (
                <QueueHours
                  queueName={selected.policy.queue_name}
                  formatError={friendly}
                  onSaved={() => void refresh()}
                />
              )}
              <div>
                <h2>Download details</h2>
                <button
                  className="icon-button"
                  aria-label="Close download details"
                  onClick={() => setSelected(null)}
                >
                  <Icon name="close" size={18} />
                </button>
              </div>
              <dl>
                <dt>Saved to</dt>
                <dd>{displayPath(selected.destination)}</dd>
                <dt>Transfer</dt>
                <dd>
                  {selected.mode === "browser"
                    ? "Browser transfer"
                    : selected.mode === "media"
                      ? "Original media tracks with local assembly"
                      : selected.mode === "ranged"
                        ? "Verified byte ranges"
                        : "Single stream"}
                </dd>
                <dt>SHA-256</dt>
                <dd>
                  {selected.mode === "browser"
                    ? "Not computed by IDMXL for browser transfers"
                    : selected.sha256 || "Available after verification"}
                </dd>
                <dt>Transfer requests / failed requests</dt>
                <dd>
                  {selected.mode === "browser"
                    ? "Managed by the browser"
                    : `${selected.transfer_requests} / ${selected.failed_requests}`}
                </dd>
                <dt>Job</dt>
                <dd>{selected.id}</dd>
              </dl>
              {selected.mode === "media" && <MediaDetails id={selected.id} />}
              {!["media", "browser"].includes(selected.mode) &&
                ["failed", "paused", "queued"].includes(selected.state) && (
                  <form
                    onSubmit={async (event) => {
                      event.preventDefault();
                      setWorking("renew");
                      try {
                        await invoke("refresh_source", {
                          id: selected.id,
                          url: renewedUrl,
                        });
                        setRenewedUrl("");
                        await refresh();
                      } catch (error) {
                        setError(friendly(error));
                      } finally {
                        setWorking("");
                      }
                    }}
                  >
                    <label>
                      Replace an expired file link
                      <input
                        type="url"
                        required
                        autoComplete="off"
                        value={renewedUrl}
                        onChange={(event) => setRenewedUrl(event.target.value)}
                        placeholder="Paste the renewed HTTP or HTTPS file link"
                      />
                    </label>
                    <p className="form-note">
                      Keeping saved bytes requires the expected SHA-256 entered
                      when adding this job and matching source details. Browser
                      captures and media need a new download. Pause other
                      downloads first.
                    </p>
                    <button
                      className="secondary"
                      disabled={working === "renew"}
                    >
                      Check and replace link
                    </button>
                  </form>
                )}
            </section>
          )}
          <footer className="page-footer">
            A fresh start for your download folder.<span>IDMXL / WINDOWS</span>
          </footer>
        </div>
      </main>

      <dialog
        ref={modalRef}
        onCancel={() => setForm(false)}
        onClose={() => setForm(false)}
      >
        <form onSubmit={(event) => void submit(event)}>
          <div className="dialog-heading">
            <div>
              <div className="eyebrow">MAKE ROOM FOR SOMETHING NEW</div>
              <h2>New download</h2>
            </div>
            <button
              type="button"
              className="icon-button"
              aria-label="Close new download"
              onClick={() => setForm(false)}
            >
              <Icon name="close" />
            </button>
          </div>
          <label>
            File URL
            <input
              autoFocus
              type="url"
              required
              maxLength={16384}
              placeholder="https://example.com/file.zip"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
            />
          </label>
          <label>
            Save as
            <div className="destination-input">
              <input
                required
                value={destination}
                onChange={(e) => setDestination(e.target.value)}
                placeholder="Choose a file location"
              />
              <button
                type="button"
                className="secondary"
                onClick={() => void chooseDestination()}
              >
                Browse
              </button>
            </div>
          </label>
          <label>
            Expected SHA-256 <small>optional</small>
            <input
              pattern="[a-fA-F0-9]{64}"
              value={digest}
              onChange={(e) => setDigest(e.target.value)}
              placeholder="A trusted hash from the file's publisher"
            />
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={replayable}
              onChange={(e) => setReplayable(e.target.checked)}
            />
            <span>
              Allow probing and parallel requests
              <small>
                For reusable file links. Leave off for one-time download links.
              </small>
            </span>
          </label>
          <p className="form-note">
            Added jobs can start automatically while the queue is running.
            Otherwise, choose Start when you’re ready. Existing files are never
            replaced automatically.
          </p>
          {error && (
            <p className="inline-error" role="alert">
              {error}
            </p>
          )}
          <div className="form-actions">
            <button
              type="button"
              className="secondary"
              onClick={() => setForm(false)}
            >
              Cancel
            </button>
            <button
              className="primary"
              type="submit"
              disabled={working === "new"}
            >
              {working === "new" ? "Adding…" : "Add download"}
              <Icon name="arrow" size={17} />
            </button>
          </div>
        </form>
      </dialog>
      <ImportLinks
        visible={showImport}
        initialLinks={importText}
        onClose={() => setShowImport(false)}
        onAdded={() => void refresh()}
        formatError={friendly}
      />
      <MediaDialog
        visible={showMedia}
        onClose={() => setShowMedia(false)}
        onAdded={() => void refresh()}
        formatError={friendly}
      />
    </div>
  );
}

function friendly(error: unknown): string {
  const code = String(error).replace(/^Error:\s*/, "");
  const messages: Record<string, string> = {
    confirmation_required: "Confirm the action before continuing.",
    media_destination_change_unsupported:
      "Choose the media output location before adding it. You can change the default folder here for future downloads.",
    destination_change_requires_pause:
      "Pause this download before changing its location.",
    delete_requires_stopped_job:
      "The download is still stopping. Wait briefly and try deleting again.",
    file_identity_changed:
      "The file at this location changed. IDMXL kept it to avoid deleting a different file.",
    file_in_use:
      "The file is in use. Close applications using it and try again.",
    file_delete_failed:
      "Windows could not delete this file. Check that it is not in use.",
    browser_control_failed:
      "The browser could not apply that action. Check its Downloads page and try again.",
    browser_disconnected:
      "The browser connection is offline. Open Chrome or Edge with IDMXL capture enabled to update this transfer.",
    browser_attention:
      "This download needs attention in the browser. IDMXL will not dismiss browser warnings or approvals.",
    browser_interrupted:
      "The browser interrupted this download. Resume it if available, or check the browser's Downloads page.",
    browser_cannot_resume:
      "The browser cannot resume this transfer. Start a new download from its page if needed.",
    source_refresh_unsupported:
      "This download cannot have its link replaced. Add the renewed link as a new download.",
    source_identity_unproven:
      "The renewed link could not prove that it is the same file. Add it as a new download to preserve your existing checkpoints.",
    retry_exhausted:
      "The retry allowance was used up. Choose Resume to allow another set of attempts.",
    notification_unavailable:
      "Windows notifications are unavailable. Your downloads will still finish normally.",
    update_in_progress:
      "An update is being prepared. Wait for IDMXL to restart.",
    update_requires_idle_app:
      "Another IDMXL process is still using the profile. Pause downloads and close other IDMXL windows, then try again.",
    invalid_update_signature:
      "The update signature could not be verified. Nothing has been installed.",
    update_rollback_rejected:
      "The update metadata is older than a release this app has already verified.",
    update_metadata_expired_or_clock_invalid:
      "The release information has expired, or this PC's clock is incorrect. Check the date and try again later.",
    signature_verification_failed:
      "Windows did not validate the expected publisher's signature. The update has not been installed.",
    media_helper_missing:
      "The media tools are missing. Install the complete IDMXL package.",
    media_helper_untrusted:
      "The media tools did not match this app version. Reinstall the complete package.",
    media_selection_changed:
      "The available tracks changed. Find the tracks again and select the exact format.",
    media_audio_selection_required:
      "Choose a separate audio track for this video rendition.",
    media_audio_group_mismatch:
      "That audio track belongs to a different video group. Choose its matching track.",
    media_extension_mismatch:
      "The filename extension must match the selected output container.",
    unsupported_media_protection_or_timeline:
      "This stream uses encryption or a timeline feature that IDMXL does not support.",
    unsupported_media_protection_or_addressing:
      "This stream uses protection or segment addressing that IDMXL does not support.",
    live_media_unsupported:
      "Live streams are not supported. Use a finished video-on-demand stream.",
    media_verification_failed:
      "The assembled media did not preserve the selected tracks or quality. The source tracks have been kept.",
    media_helper_failed:
      "The media could not be combined without conversion. Try a compatible output container.",
    browser_handoff_pending:
      "The browser handoff has not been committed. Keep this download in the browser until it finishes.",
    destination_exists:
      "A file already exists at that location. Choose a different name.",
    destination_in_use:
      "Another download already uses that filename. Choose a different name.",
    invalid_queue_name: "Enter a queue name of up to 64 characters.",
    invalid_queue_window:
      "Select at least one weekday and valid start and end times.",
    invalid_folder_settings:
      "Choose an existing download folder and up to 20 folder rules.",
    invalid_folder_rule:
      "Use unique folder names and file extensions. Folder names cannot contain paths or reserved Windows names.",
    category_folder_unavailable:
      "A category folder could not be used. Choose another folder or turn off organization.",
    invalid_batch_size: "Enter between 1 and 1,000 links, up to 1 MB in total.",
    download_could_not_be_added:
      "This download could not be added. Check the folder and link.",
    no_available_filename:
      "No free filename could be chosen. Use another folder.",
    url_too_large: "This file link is too long.",
    destination_folder_missing:
      "That folder does not exist. Choose an existing folder.",
    invalid_filename:
      "That filename is not supported on Windows. Choose another name.",
    invalid_url: "Enter a valid file URL.",
    unsupported_protocol: "Use an HTTP or HTTPS file link.",
    engine_busy:
      "A download is already running. Pause it before starting another.",
    resource_changed:
      "The source file changed. Start again to fetch a consistent version.",
    digest_mismatch:
      "The file did not match the expected SHA-256. It has not been published.",
    server_throttled: "The server asked us to wait. Try this download later.",
    server_retry_pending:
      "The source is cooling down as requested by its server. Queued jobs wait until that deadline.",
    private_network_blocked:
      "Browser handoffs can reach public internet addresses only. Add an intranet link directly in the app if intended.",
    invalid_queue_policy:
      "Check the queue name, times, connection count, priority and speed limit.",
    interrupted: "The previous session ended. Resume to verify and continue.",
    publication_failed:
      "The final file could not be saved. Check its destination and try again.",
    publication_identity_mismatch:
      "The destination has changed. Your existing file has been preserved.",
    url_credentials_unsupported:
      "Password-bearing URLs are not supported in this preview.",
    invalid_sha256: "SHA-256 must contain exactly 64 hexadecimal characters.",
    engine_binary_missing:
      "The download engine is missing. Build IDMXL using the repository's desktop script.",
  };
  return (
    messages[code] ??
    "This operation could not finish. Your existing files have been preserved. Technical code: " +
      code
  );
}

createRoot(document.getElementById("root")!).render(<App />);
