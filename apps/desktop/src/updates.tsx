import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
type Result = {
  configured: boolean;
  available: boolean;
  version: string | null;
  notes: string | null;
};
export function Updates({
  formatError,
}: {
  formatError: (error: unknown) => string;
}) {
  const [result, setResult] = useState<Result | null>(null),
    [busy, setBusy] = useState(false),
    [installing, setInstalling] = useState(false),
    [error, setError] = useState("");
  async function check() {
    setBusy(true);
    setError("");
    try {
      setResult(await invoke<Result>("check_updates"));
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }
  async function install() {
    if (!result?.version) return;
    setBusy(true);
    setInstalling(true);
    setError("");
    try {
      await invoke("install_update", { version: result.version });
    } catch (e) {
      setError(formatError(e));
      setInstalling(false);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="update-panel">
      <button
        className="secondary"
        onClick={() => void check()}
        disabled={busy}
      >
        {busy ? "Working…" : "Check for updates"}
      </button>
      {result && !result.configured && (
        <p>
          Online updates are not configured for this development build. Release
          signing and HTTPS hosting must be set up first.
        </p>
      )}
      {result?.configured && !result.available && (
        <p>You have the current published version.</p>
      )}
      {result?.available && (
        <>
          <p>Version {result.version} is available.</p>
          <p>{result.notes}</p>
          <p>
            Installation pauses downloads, saves a recovery copy of the app and
            history, then restarts IDMXL.
          </p>
          <button
            className="primary"
            disabled={busy}
            onClick={() => void install()}
          >
            Download and install update
          </button>
        </>
      )}
      {installing && (
        <p role="status">
          Pausing downloads, checking the signed installer and preparing
          recovery…
        </p>
      )}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
