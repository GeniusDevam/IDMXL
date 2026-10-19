import React, { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { displayPath } from "./paths";
type Choice = { id: string; label: string; role: string };
type Inspection = { kind: string; choices: Choice[] };
export function MediaDialog({
  visible,
  onClose,
  onAdded,
  formatError,
}: {
  visible: boolean;
  onClose: () => void;
  onAdded: () => void;
  formatError: (error: unknown) => string;
}) {
  const dialog = useRef<HTMLDialogElement>(null),
    oldFocus = useRef<HTMLElement | null>(null);
  const [url, setUrl] = useState(""),
    [audioUrl, setAudioUrl] = useState(""),
    [destination, setDestination] = useState(""),
    [selection, setSelection] = useState(""),
    [audio, setAudio] = useState(""),
    [container, setContainer] = useState("mkv");
  const [inspection, setInspection] = useState<Inspection | null>(null),
    [audioOnly, setAudioOnly] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    if (visible) {
      oldFocus.current = document.activeElement as HTMLElement;
      dialog.current?.showModal();
      setError("");
    } else {
      dialog.current?.close();
      oldFocus.current?.focus();
    }
  }, [visible]);
  useEffect(() => {
    setInspection(null);
    setSelection("");
    setAudio("");
  }, [url]);
  async function inspect() {
    setBusy(true);
    setError("");
    try {
      const value = await invoke<Inspection>("inspect_media", { url });
      setInspection(value);
      setSelection(
        value.choices.find((c) => c.role !== "audio")?.id ||
          value.choices[0]?.id ||
          "",
      );
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }
  async function choose() {
    const folder = displayPath(await invoke<string>("download_location"));
    const path = await save({
      title: "Save media as",
      defaultPath: `${folder}\\video.${container}`,
      filters: [{ name: container.toUpperCase(), extensions: [container] }],
    });
    if (path) setDestination(path);
  }
  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await invoke("add_media", {
        request: {
          url,
          selection,
          audio_selection: audio || null,
          audio_url: audioUrl || null,
          destination,
          container,
          audio_only: audioOnly,
        },
      });
      setUrl("");
      setAudioUrl("");
      setDestination("");
      onAdded();
      onClose();
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <dialog
      ref={dialog}
      className="workflow-dialog"
      onCancel={(e) => {
        if (busy) e.preventDefault();
        else onClose();
      }}
    >
      <form onSubmit={(e) => void submit(e)}>
        <div className="dialog-heading">
          <div>
            <h2>Download media</h2>
            <p>Keep the selected quality and codecs.</p>
          </div>
          <button
            type="button"
            className="icon-button"
            aria-label="Close media download"
            disabled={busy}
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <p>
          Use a direct media file, an HLS VOD link (.m3u8), or a static DASH
          link (.mpd). Website page links, encrypted media and live streams are
          not supported.
        </p>
        <label>
          Media link
          <input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            type="url"
            required
            disabled={busy}
            autoFocus
            autoComplete="off"
          />
        </label>
        <button
          type="button"
          className="secondary"
          disabled={busy || !url}
          onClick={() => void inspect()}
        >
          {busy ? "Working…" : "Find original tracks"}
        </button>
        {inspection && (
          <>
            <label>
              Video or main track
              <select
                value={selection}
                onChange={(e) => setSelection(e.target.value)}
                disabled={busy}
              >
                {inspection.choices.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.label}
                  </option>
                ))}
              </select>
            </label>
            {inspection.choices.some((c) => c.role === "audio") && (
              <label>
                Separate audio track
                <select
                  value={audio}
                  onChange={(e) => setAudio(e.target.value)}
                  disabled={busy}
                >
                  <option value="">Use main track audio, if present</option>
                  {inspection.choices
                    .filter((c) => c.role === "audio")
                    .map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.label}
                      </option>
                    ))}
                </select>
              </label>
            )}
            {inspection.kind === "direct" && (
              <label>
                Separate audio file link (optional)
                <input
                  type="url"
                  value={audioUrl}
                  onChange={(e) => setAudioUrl(e.target.value)}
                  disabled={busy || audioOnly}
                  autoComplete="off"
                />
              </label>
            )}
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={audioOnly}
                onChange={(e) => {
                  setAudioOnly(e.target.checked);
                  if (e.target.checked) setAudioUrl("");
                }}
                disabled={busy}
              />
              Save audio only
            </label>
            <label>
              Output container
              <select
                value={container}
                onChange={(e) => {
                  setContainer(e.target.value);
                  setDestination("");
                }}
                disabled={busy}
              >
                {["mkv", "mp4", "webm", "mov", "m4a", "mp3", "wav", "ogg"].map(
                  (c) => (
                    <option key={c}>{c}</option>
                  ),
                )}
              </select>
            </label>
            <p>
              Stream copy does not convert codecs. Choose a compatible
              container; IDMXL will stop if tracks or quality cannot be
              preserved. Byte-range manifests currently download each complete
              source file before slicing locally.
            </p>
            <label>
              Save as
              <div className="input-action">
                <input
                  value={destination}
                  onChange={(e) => setDestination(e.target.value)}
                  required
                  disabled={busy}
                />
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void choose()}
                  disabled={busy}
                >
                  Browse…
                </button>
              </div>
            </label>
          </>
        )}
        {error && (
          <p className="error-message" role="alert">
            {error}
          </p>
        )}
        <div className="dialog-actions">
          <button
            type="button"
            className="secondary"
            onClick={onClose}
            disabled={busy}
          >
            Cancel
          </button>
          <button
            className="primary"
            type="submit"
            disabled={busy || !inspection || !destination || !selection}
          >
            Add to queue
          </button>
        </div>
      </form>
    </dialog>
  );
}

export function MediaDetails({ id }: { id: string }) {
  const [details, setDetails] = useState<{
    container: string;
    audio_only: boolean;
    tracks: {
      role: string;
      width: number | null;
      height: number | null;
      segments: number;
      duration: number | null;
    }[];
  } | null>(null);
  useEffect(() => {
    let alive = true;
    void invoke<typeof details>("media_details", { id })
      .then((value) => {
        if (alive) setDetails(value);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [id]);
  if (!details) return null;
  return (
    <div className="media-details">
      <strong>Selected original media</strong>
      <p>
        {details.container.toUpperCase()} ·{" "}
        {details.audio_only ? "Audio only" : "Stream copy"}
      </p>
      {details.tracks.map((t, i) => (
        <p key={i}>
          {t.role}
          {t.width && t.height ? ` · ${t.width} × ${t.height}` : ""} ·{" "}
          {t.segments} source {t.segments === 1 ? "piece" : "pieces"}
          {t.duration ? ` · ${Math.round(t.duration)} seconds` : ""}
        </p>
      ))}
    </div>
  );
}
