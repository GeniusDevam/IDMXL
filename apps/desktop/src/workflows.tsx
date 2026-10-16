import React, { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as chooseFolder } from "@tauri-apps/plugin-dialog";
import { displayPath } from "./paths";

type FolderRule = { folder: string; extensions: string[] };
type Folders = { directory: string; organize: boolean; rules: FolderRule[] };
type LinkResult = {
  index: number;
  status: "queued" | "duplicate" | "rejected";
  job?: { name: string; destination: string };
  error?: string;
};
type WindowHours = { days: number; start_minute: number; end_minute: number };
type ErrorFormatter = (error: unknown) => string;
function readableFolders(folders: Folders): Folders {
  return { ...folders, directory: displayPath(folders.directory) };
}

export function ImportLinks({
  visible,
  initialLinks,
  onClose,
  onAdded,
  formatError,
}: {
  visible: boolean;
  initialLinks?: string;
  onClose: () => void;
  onAdded: () => void;
  formatError: ErrorFormatter;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const oldFocus = useRef<HTMLElement | null>(null);
  const [links, setLinks] = useState("");
  const [folders, setFolders] = useState<Folders>({
    directory: "",
    organize: true,
    rules: [],
  });
  const [queue, setQueue] = useState("Default");
  const [replayable, setReplayable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [results, setResults] = useState<LinkResult[]>([]);
  const lines = links
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

  useEffect(() => {
    if (visible && initialLinks) setLinks(initialLinks);
  }, [visible, initialLinks]);

  useEffect(() => {
    if (!visible) {
      dialog.current?.close();
      oldFocus.current?.focus();
      return;
    }
    oldFocus.current = document.activeElement as HTMLElement;
    dialog.current?.showModal();
    setError("");
    setNotice("");
    setResults([]);
    setLoading(true);
    let cancelled = false;
    void invoke<{ folders: Folders | null; default_rules: FolderRule[] }>(
      "workflow_settings",
    )
      .then((value) => {
        if (!cancelled)
          setFolders(
            value.folders
              ? readableFolders(value.folders)
              : { directory: "", organize: true, rules: value.default_rules },
          );
      })
      .catch((error) => {
        if (!cancelled) setError(formatError(error));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [visible]);

  async function browse() {
    try {
      const directory = await chooseFolder({
        title: "Choose download folder",
        directory: true,
        multiple: false,
      });
      if (typeof directory === "string") setFolders({ ...folders, directory });
    } catch (error) {
      setError(formatError(error));
    }
  }

  async function saveFolders() {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      setFolders(
        readableFolders(
          await invoke<Folders>("save_folder_settings", { settings: folders }),
        ),
      );
      setNotice("Folder preferences saved.");
    } catch (error) {
      setError(formatError(error));
    } finally {
      setBusy(false);
    }
  }

  async function add(event: React.FormEvent) {
    event.preventDefault();
    setError("");
    setNotice("");
    setResults([]);
    if (
      lines.length === 0 ||
      lines.length > 1000 ||
      new TextEncoder().encode(links).length > 1_048_576
    ) {
      setError("Enter between 1 and 1,000 links, one per line (up to 1 MB).");
      return;
    }
    setBusy(true);
    try {
      const saved = await invoke<Folders>("save_folder_settings", {
        settings: folders,
      });
      setFolders(readableFolders(saved));
      const value = await invoke<{ results: LinkResult[] }>("add_links", {
        batch: { links: lines, folders: saved, replayable, queue_name: queue },
      });
      setResults(value.results);
      // Retain only rejected lines, so correcting them cannot resubmit accepted entries.
      setLinks(
        value.results
          .filter((result) => result.status === "rejected")
          .map((result) => lines[result.index])
          .join("\n"),
      );
      setNotice(
        `${value.results.filter((result) => result.status === "queued").length} added; ${value.results.filter((result) => result.status === "duplicate").length} duplicate links skipped; ${value.results.filter((result) => result.status === "rejected").length} need attention.`,
      );
      onAdded();
    } catch (error) {
      setError(formatError(error));
    } finally {
      setBusy(false);
    }
  }

  function editRule(index: number, patch: Partial<FolderRule>) {
    setFolders({
      ...folders,
      rules: folders.rules.map((rule, at) =>
        at === index ? { ...rule, ...patch } : rule,
      ),
    });
  }

  return (
    <dialog
      ref={dialog}
      className="import-dialog"
      onCancel={(event) => {
        if (busy) event.preventDefault();
        else onClose();
      }}
    >
      <div className="dialog-heading">
        <div>
          <p className="eyebrow">ONE LINK OR A WHOLE LIST</p>
          <h2>Import download links</h2>
        </div>
        <button
          className="icon-button"
          type="button"
          aria-label="Close link import"
          disabled={busy}
          onClick={onClose}
        >
          ×
        </button>
      </div>
      <form onSubmit={(event) => void add(event)}>
        <fieldset disabled={busy || loading}>
          <label>
            File links, one per line
            <textarea
              autoFocus
              rows={5}
              maxLength={1_048_576}
              value={links}
              onChange={(event) => setLinks(event.target.value)}
              placeholder={
                "https://example.com/file.zip\nhttps://example.com/report.pdf"
              }
            />
          </label>
          <p className="form-note">
            {lines.length} links. Direct HTTP/HTTPS file links only. Duplicate
            links in this list are skipped. Filenames come from the links;
            existing files are kept and numbered names are used when needed.
          </p>
          <label>
            Download folder
            <div className="destination-input">
              <input
                required
                value={folders.directory}
                onChange={(event) =>
                  setFolders({ ...folders, directory: event.target.value })
                }
                placeholder="Choose an existing folder"
              />
              <button
                className="secondary"
                type="button"
                onClick={() => void browse()}
              >
                Browse folders
              </button>
            </div>
          </label>
          <label>
            Queue name
            <input
              required
              maxLength={64}
              value={queue}
              onChange={(event) => setQueue(event.target.value)}
            />
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={folders.organize}
              onChange={(event) =>
                setFolders({ ...folders, organize: event.target.checked })
              }
            />
            <span>
              Organize into folders by file type
              <small>
                Unmatched files go into Other. These rules apply to links added
                here.
              </small>
            </span>
          </label>
          <details className="folder-rules">
            <summary>Folder rules</summary>
            <p className="form-note">
              Each extension belongs to one folder. Use commas between
              extensions, for example pdf, docx.
            </p>
            {folders.rules.map((rule, index) => (
              <div className="folder-rule" key={index}>
                <label>
                  Folder {index + 1}
                  <input
                    value={rule.folder}
                    maxLength={80}
                    onChange={(event) =>
                      editRule(index, { folder: event.target.value })
                    }
                  />
                </label>
                <label>
                  Extensions {index + 1}
                  <input
                    value={rule.extensions.join(",")}
                    onChange={(event) =>
                      editRule(index, {
                        extensions: event.target.value.split(","),
                      })
                    }
                  />
                </label>
                <button
                  className="secondary"
                  type="button"
                  onClick={() =>
                    setFolders({
                      ...folders,
                      rules: folders.rules.filter((_, at) => at !== index),
                    })
                  }
                  aria-label={`Remove folder rule ${index + 1}`}
                >
                  Remove rule
                </button>
              </div>
            ))}
            <button
              className="secondary"
              type="button"
              disabled={folders.rules.length >= 20}
              onClick={() =>
                setFolders({
                  ...folders,
                  rules: [...folders.rules, { folder: "", extensions: [] }],
                })
              }
            >
              Add folder rule
            </button>
          </details>
          <button
            className="secondary"
            type="button"
            onClick={() => void saveFolders()}
          >
            Save folder preferences
          </button>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={replayable}
              onChange={(event) => setReplayable(event.target.checked)}
            />
            <span>
              Allow probing and parallel requests
              <small>
                Use only for reusable file links. Leave off for one-time links.
              </small>
            </span>
          </label>
          <p className="form-note">
            Added jobs can start while the queue is running. A queue's recurring
            hours apply automatically. This imports file links; it does not scan
            pages or discover videos.
          </p>
        </fieldset>
        {error && (
          <p className="inline-error" role="alert">
            {error}
          </p>
        )}
        {notice && (
          <p className="workflow-notice" role="status">
            {notice}
          </p>
        )}
        {results.length > 0 && (
          <ol className="import-results" aria-label="Import results">
            {results.map((result) => (
              <li key={result.index} value={result.index + 1}>
                {result.status === "queued" ? (
                  <>
                    <strong>Added: {result.job?.name}</strong>
                    <small>
                      {result.job ? displayPath(result.job.destination) : ""}
                    </small>
                  </>
                ) : result.status === "duplicate" ? (
                  "Duplicate link skipped"
                ) : (
                  formatError(result.error)
                )}
              </li>
            ))}
          </ol>
        )}
        <div className="form-actions">
          <button
            className="secondary"
            type="button"
            disabled={busy}
            onClick={onClose}
          >
            Close
          </button>
          <button
            className="primary"
            type="submit"
            disabled={busy || loading || lines.length === 0}
          >
            {busy
              ? "Working…"
              : `Add ${lines.length || ""} ${lines.length === 1 ? "link" : "links"}`}
          </button>
        </div>
      </form>
    </dialog>
  );
}

const weekdays = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
];
function timeText(minutes: number) {
  return `${Math.floor(minutes / 60)
    .toString()
    .padStart(2, "0")}:${(minutes % 60).toString().padStart(2, "0")}`;
}
function timeMinutes(value: string) {
  const [hours, minutes] = value.split(":").map(Number);
  return hours * 60 + minutes;
}

export function QueueHours({
  queueName,
  formatError,
  onSaved,
}: {
  queueName: string;
  formatError: ErrorFormatter;
  onSaved: () => void;
}) {
  const [enabled, setEnabled] = useState(false);
  const [hours, setHours] = useState<WindowHours>({
    days: 127,
    start_minute: 22 * 60,
    end_minute: 6 * 60,
  });
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError("");
    setSaved(false);
    void invoke<{ window: WindowHours | null }>("get_queue_window", {
      queueName,
    })
      .then((value) => {
        if (!cancelled) {
          setEnabled(Boolean(value.window));
          setHours(
            value.window ?? {
              days: 127,
              start_minute: 22 * 60,
              end_minute: 6 * 60,
            },
          );
        }
      })
      .catch((error) => {
        if (!cancelled) setError(formatError(error));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [queueName]);
  async function save(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    setSaved(false);
    try {
      await invoke("save_queue_window", {
        queueName,
        window: enabled ? hours : null,
      });
      setSaved(true);
      onSaved();
    } catch (error) {
      setError(formatError(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <form className="queue-hours" onSubmit={(event) => void save(event)}>
      <h3>Recurring hours for {queueName}</h3>
      <fieldset disabled={busy || loading}>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(event) => {
              setEnabled(event.target.checked);
              setSaved(false);
            }}
          />
          <span>Run this queue during recurring hours</span>
        </label>
        {enabled && (
          <>
            <div
              className="weekday-options"
              role="group"
              aria-label="Days this queue can start downloads"
            >
              {weekdays.map((day, index) => (
                <label key={day}>
                  <input
                    type="checkbox"
                    checked={Boolean(hours.days & (1 << index))}
                    onChange={(event) => {
                      setSaved(false);
                      setHours({
                        ...hours,
                        days: event.target.checked
                          ? hours.days | (1 << index)
                          : hours.days & ~(1 << index),
                      });
                    }}
                  />
                  {day}
                </label>
              ))}
            </div>
            <div className="hours-inputs">
              <label>
                Start time
                <input
                  type="time"
                  required
                  value={timeText(hours.start_minute)}
                  onChange={(event) => {
                    if (event.target.value) {
                      setSaved(false);
                      setHours({
                        ...hours,
                        start_minute: timeMinutes(event.target.value),
                      });
                    }
                  }}
                />
              </label>
              <label>
                End time
                <input
                  type="time"
                  required
                  value={timeText(hours.end_minute)}
                  onChange={(event) => {
                    if (event.target.value) {
                      setSaved(false);
                      setHours({
                        ...hours,
                        end_minute: timeMinutes(event.target.value),
                      });
                    }
                  }}
                />
              </label>
            </div>
          </>
        )}
        <p className="form-note">
          Uses this computer's local time (
          {Intl.DateTimeFormat().resolvedOptions().timeZone}). Overnight hours
          continue into the following day; equal start and end times mean all
          day. The queue worker must be running while you are signed in.
          Completed files are not downloaded again.
        </p>
        <p className="form-note">
          When hours end, segmented downloads stop after a verified checkpoint.
          Single-stream downloads finish their current file. Start or resume on
          an individual file runs it immediately outside these hours.
        </p>
        <button className="secondary" type="submit">
          {busy ? "Saving…" : "Save recurring hours"}
        </button>
      </fieldset>
      {error && (
        <p className="inline-error" role="alert">
          {error}
        </p>
      )}
      {saved && (
        <p className="workflow-notice" role="status">
          Recurring hours saved.
        </p>
      )}
    </form>
  );
}
