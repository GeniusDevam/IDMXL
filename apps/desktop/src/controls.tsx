import { useEffect, useId, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as chooseFolder, save } from "@tauri-apps/plugin-dialog";
import { displayPath } from "./paths";

type ManagedJob = {
  id: string;
  name: string;
  state: string;
  destination: string;
  mode: string;
  file_available: boolean;
};
type Confirmation = {
  title: string;
  description: string;
  confirmLabel: string;
  run: () => Promise<void>;
};
type Props = { onChanged: () => void; formatError: (error: unknown) => string };

export function ConfirmAction({
  request,
  onClose,
  onError,
}: {
  request: Confirmation | null;
  onClose: () => void;
  onError: (error: unknown) => void;
}) {
  const titleId = useId();
  const dialog = useRef<HTMLDialogElement>(null),
    cancel = useRef<HTMLButtonElement>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (request) {
      dialog.current?.showModal();
      cancel.current?.focus();
    } else dialog.current?.close();
  }, [request]);
  return (
    <dialog
      ref={dialog}
      className="workflow-dialog"
      aria-labelledby={titleId}
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) onClose();
      }}
    >
      <div className="dialog-heading">
        <h2 id={titleId}>{request?.title}</h2>
        <button
          type="button"
          className="icon-button"
          aria-label="Close confirmation"
          disabled={busy}
          onClick={onClose}
        >
          ×
        </button>
      </div>
      <p>{request?.description}</p>
      <div className="form-actions">
        <button
          ref={cancel}
          type="button"
          className="secondary"
          disabled={busy}
          onClick={onClose}
        >
          Cancel
        </button>
        <button
          type="button"
          className="primary"
          disabled={busy || !request}
          onClick={async () => {
            if (!request || busy) return;
            setBusy(true);
            try {
              await request.run();
              onClose();
            } catch (e) {
              onError(e);
              onClose();
            } finally {
              setBusy(false);
            }
          }}
        >
          {busy ? "Working…" : request?.confirmLabel}
        </button>
      </div>
    </dialog>
  );
}

async function manage(jobs: ManagedJob[], action: string, props: Props) {
  const results = await invoke<{ success: boolean; error: string | null }[]>(
    "manage_downloads",
    { ids: jobs.map((j) => j.id), action, confirmed: true },
  );
  props.onChanged();
  const failed = results.filter((r) => !r.success);
  if (failed.length)
    throw new Error(
      `${failed.length} action(s) could not finish. ${props.formatError(failed[0].error)}`,
    );
}

export function DownloadManagement({
  jobs,
  ...props
}: Props & { jobs: ManagedJob[] }) {
  const [request, setRequest] = useState<Confirmation | null>(null),
    [folder, setFolder] = useState(""),
    [error, setError] = useState("");
  useEffect(() => {
    void invoke<string>("download_location")
      .then((p) => setFolder(displayPath(p)))
      .catch((e) =>
        setError(e instanceof Error ? e.message : props.formatError(e)),
      );
  }, []);
  function bulk(action: "pause" | "cancel" | "delete") {
    const targets = jobs.filter(
      (j) =>
        action === "delete" ||
        (action === "pause"
          ? [
              "queued",
              "paused",
              "downloading",
              "pausing",
              "verifying",
            ].includes(j.state)
          : !["completed", "canceled", "publishing"].includes(j.state)),
    );
    if (!targets.length) return;
    setRequest({
      title: `${action === "pause" ? "Pause" : action === "cancel" ? "Cancel" : "Delete files for"} all ${targets.length} downloads?`,
      description:
        action === "delete"
          ? "This stops the selected downloads and permanently deletes their downloaded files and partial files from this PC. History entries stay visible. Browser file actions require the browser to be connected. Cancel or close this window to keep the files."
          : action === "cancel"
            ? "This cancels these downloads and stops the IDMXL queue. Native partial files are kept; browser transfers follow Chrome or Edge's cancellation behavior."
            : "This pauses these downloads and stops the IDMXL queue. You can resume them later.",
      confirmLabel:
        action === "delete"
          ? "Confirm deletion"
          : action === "cancel"
            ? "Cancel all downloads"
            : "Pause all downloads",
      run: () => manage(targets, action, props),
    });
  }
  return (
    <section className="download-management" aria-label="Download management">
      <div>
        <strong>Download location</strong>
        <p className="form-note">{folder || "Windows Downloads"}</p>
        <button
          className="secondary"
          onClick={async () => {
            try {
              const chosen = await chooseFolder({
                title: "Choose default download location",
                directory: true,
                multiple: false,
                defaultPath: folder || undefined,
              });
              if (typeof chosen === "string")
                setFolder(
                  displayPath(
                    await invoke<string>("download_location", {
                      directory: chosen,
                    }),
                  ),
                );
            } catch (e) {
              setError(e instanceof Error ? e.message : props.formatError(e));
            }
          }}
        >
          Change default folder
        </button>
        <p className="form-note">
          Each native download asks where to save. A folder becomes the
          suggested default after four confirmed uses.
        </p>
      </div>
      <div className="heading-actions">
        <button
          className="secondary"
          disabled={!jobs.length}
          onClick={() => bulk("pause")}
        >
          Pause all
        </button>
        <button
          className="secondary"
          disabled={!jobs.length}
          onClick={() => bulk("cancel")}
        >
          Cancel all
        </button>
        <button
          className="secondary danger"
          disabled={!jobs.length}
          onClick={() => bulk("delete")}
        >
          Delete all files
        </button>
      </div>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <ConfirmAction
        request={request}
        onClose={() => setRequest(null)}
        onError={(e) =>
          setError(e instanceof Error ? e.message : props.formatError(e))
        }
      />
    </section>
  );
}

export function FileControls({ job, ...props }: Props & { job: ManagedJob }) {
  const [request, setRequest] = useState<Confirmation | null>(null),
    [error, setError] = useState("");
  const canMove =
    job.mode !== "browser" &&
    ["queued", "paused", "failed", "canceled"].includes(job.state);
  return (
    <div className="file-controls">
      <strong>File location</strong>
      <p className="form-note">
        {job.mode === "browser"
          ? "Chosen in Chrome or Edge. Browser transfers use the browser's Save As dialog."
          : displayPath(job.destination)}
      </p>
      {canMove && (
        <button
          className="secondary"
          onClick={async () => {
            try {
              const chosen = await save({
                title: "Change download location",
                defaultPath: displayPath(job.destination),
              });
              if (chosen) {
                await invoke("change_destination", {
                  id: job.id,
                  destination: chosen,
                });
                props.onChanged();
              }
            } catch (e) {
              setError(e instanceof Error ? e.message : props.formatError(e));
            }
          }}
        >
          Change download location
        </button>
      )}
      {!canMove && job.mode !== "browser" && job.state !== "completed" && (
        <p className="form-note">
          Pause the download before changing its location.
        </p>
      )}
      <button
        className="secondary danger"
        onClick={() =>
          setRequest({
            title: `Delete ${job.name}?`,
            description:
              "This stops this download and permanently deletes its downloaded file or partial file from this PC. Its history entry stays visible. Cancel or close this window to keep the file.",
            confirmLabel: "Confirm deletion",
            run: () => manage([job], "delete", props),
          })
        }
      >
        Delete file from PC
      </button>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <ConfirmAction
        request={request}
        onClose={() => setRequest(null)}
        onError={(e) =>
          setError(e instanceof Error ? e.message : props.formatError(e))
        }
      />
    </div>
  );
}
