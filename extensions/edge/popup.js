import {
  CAPTURE_PERMISSIONS,
  enableAutomaticCapture,
  disableAutomaticCapture,
} from "./capture-settings.js";
import { captureReason } from "./capture-diagnostics.js";

chrome.storage.local.get("lastStatus").then(({ lastStatus }) => {
  if (lastStatus) document.getElementById("status").textContent = lastStatus;
});
document.getElementById("check-host").addEventListener("click", async () => {
  const status = document.getElementById("status");
  try {
    const reply = await chrome.runtime.sendNativeMessage("dev.idmxl.host", {
      version: 2,
      kind: "status",
      request_id: crypto.randomUUID(),
    });
    status.textContent =
      reply?.error === "handoff_not_found"
        ? "IDMXL is connected and ready."
        : "IDMXL replied. Check the installed engine version.";
  } catch (error) {
    const message = String(error?.message || "");
    status.textContent = /not found/i.test(message)
      ? "IDMXL host was not found. Run browser-host.ps1 for this browser and extension ID."
      : /forbidden|access/i.test(message)
        ? "This extension is not allowed by the host registration. Register its exact ID."
        : /exited/i.test(message)
          ? "IDMXL host exited before replying. Check its profile and download folder setup."
          : /communicat|start|launch/i.test(message)
            ? "The browser could not start or communicate with the IDMXL host. Check its executable path."
            : "IDMXL connection failed. Check the browser-host registration.";
  }
});
async function showRules() {
  const { captureConfig } = await chrome.storage.local.get("captureConfig");
  const permitted = await chrome.permissions.contains(CAPTURE_PERMISSIONS);
  let worker;
  try { worker = await chrome.runtime.sendMessage({ kind: "capture-state" }); } catch {}
  const enabled = captureConfig?.enabled && captureConfig.scope === "all" && permitted && worker?.ready;
  document.getElementById("rules").textContent = enabled
    ? "On: native downloads and Browser transfer compatibility across websites."
    : captureConfig?.enabled && permitted && captureConfig.scope === "all"
      ? "Capture is enabled in settings, but its worker is not ready. Click Enable automatic capture to reconnect it."
    : captureConfig?.enabled && captureConfig.scope !== "all"
      ? "Folder-only capture is active. Enable automatic capture to use all websites."
      : "Automatic capture is off. Enable it to grant browser access.";
  document.getElementById("enable-capture").disabled = enabled;
  document.getElementById("disable").disabled = !captureConfig?.enabled;
  const { lastCaptureAttempt } = await chrome.storage.local.get("lastCaptureAttempt");
  const { captureNoticeError } = await chrome.storage.local.get("captureNoticeError");
  const { browserBridgeError } = await chrome.storage.local.get("browserBridgeError");
  document.getElementById("capture-detail").textContent =
    (worker?.ready ? "Capture worker is running. " : "Capture worker is not running. ") +
    captureReason(lastCaptureAttempt?.code) +
    (captureNoticeError ? " The confirmation window could not open; check IDMXL's history for the captured job." : "") +
    (browserBridgeError ? " " + browserBridgeError : "");
}
void showRules();
document.getElementById("enable-capture").addEventListener("click", async () => {
  const status = document.getElementById("status");
  try {
    const granted = await enableAutomaticCapture(chrome);
    if (!granted) {
      status.textContent = "Capture permission was not granted.";
      return;
    }
    status.textContent = "Automatic capture is on. Compatible files use IDMXL; other files appear as Browser transfers using the original browser session.";
    await showRules();
  } catch {
    status.textContent = "Could not enable automatic capture. Check the extension's browser permissions and try again.";
    await showRules();
  }
});
document.getElementById("refresh-capture").addEventListener("click", showRules);

document.getElementById("disable").addEventListener("click", async () => {
  await disableAutomaticCapture(chrome);
  await showRules();
});
let mediaRequestId = null;
const mediaStatus = document.getElementById("status");
const sources = document.getElementById("media-sources"),
  tracks = document.getElementById("media-tracks"),
  audioTracks = document.getElementById("media-audio");
function option(select, value, label) {
  const item = document.createElement("option");
  item.value = value;
  item.textContent = label;
  select.appendChild(item);
}
document.getElementById("find-media").addEventListener("click", async () => {
  sources.replaceChildren();
  tracks.replaceChildren();
  document.getElementById("queue-media").disabled = true;
  try {
    const [tab] = await chrome.tabs.query({
      active: true,
      currentWindow: true,
    });
    if (!tab?.id || tab.incognito) throw new Error();
    const results = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: () =>
        [...document.querySelectorAll("video,audio")]
          .flatMap((media) => [
            media.currentSrc,
            media.src,
            ...[...media.querySelectorAll("source")].map((s) => s.src),
          ])
          .filter(Boolean)
          .slice(0, 100),
    });
    const urls = [...new Set(results.flatMap((r) => r.result || []))]
      .filter((value) => {
        try {
          const u = new URL(value);
          return (
            ["http:", "https:"].includes(u.protocol) &&
            !u.username &&
            !u.password
          );
        } catch {
          return false;
        }
      })
      .slice(0, 20);
    for (const value of urls) {
      const u = new URL(value);
      option(sources, value, u.origin + u.pathname);
    }
    document.getElementById("inspect-media").disabled = !urls.length;
    mediaStatus.textContent = urls.length
      ? "Choose a public media source to inspect."
      : "No transferable media link was found. Blob-based players and signed-in streams are unsupported.";
  } catch {
    mediaStatus.textContent =
      "This tab cannot be inspected. Use a public media page in an ordinary browser tab.";
  }
});
sources.addEventListener("change", () => {
  mediaRequestId = null;
  document.getElementById("queue-media").disabled = true;
});
document.getElementById("inspect-media").addEventListener("click", async () => {
  if (!sources.value) return;
  mediaRequestId = crypto.randomUUID();
  tracks.replaceChildren();
  audioTracks.replaceChildren();
  option(audioTracks, "", "Main track audio");
  document.getElementById("queue-media").disabled = true;
  try {
    const reply = await chrome.runtime.sendNativeMessage("dev.idmxl.host", {
      version: 3,
      kind: "media_inspect",
      request_id: mediaRequestId,
      url: sources.value,
    });
    if (
      reply.version !== 3 ||
      !reply.accepted ||
      reply.request_id !== mediaRequestId ||
      !Array.isArray(reply.inspection?.choices)
    )
      throw new Error();
    for (const choice of reply.inspection.choices) {
      option(tracks, choice.id, choice.label);
      if (choice.role === "audio") option(audioTracks, choice.id, choice.label);
    }
    const videoChoice = reply.inspection.choices.find(
      (choice) => choice.role !== "audio",
    );
    if (videoChoice) tracks.value = videoChoice.id;
    document.getElementById("queue-media").disabled = !tracks.options.length;
    mediaStatus.textContent =
      "Choose exact tracks and a compatible container. No conversion will be performed.";
  } catch {
    mediaStatus.textContent =
      "Media discovery failed. This source may be unsupported, private, protected or unavailable.";
  }
});
document.getElementById("queue-media").addEventListener("click", async () => {
  if (!mediaRequestId || !sources.value || !tracks.value) return;
  const button = document.getElementById("queue-media");
  button.disabled = true;
  try {
    const reply = await chrome.runtime.sendNativeMessage("dev.idmxl.host", {
      version: 3,
      kind: "media_enqueue",
      request_id: mediaRequestId,
      url: sources.value,
      filename: "media",
      selection: tracks.value,
      audio_selection: audioTracks.value || null,
      container: document.getElementById("media-container").value,
      audio_only: document.getElementById("media-audio-only").checked,
    });
    if (
      reply.version !== 3 ||
      !reply.accepted ||
      reply.request_id !== mediaRequestId ||
      !reply.job_id
    )
      throw new Error();
    mediaStatus.textContent =
      "Selected media queued in IDMXL. Open IDMXL to start it, or leave its queue running.";
  } catch {
    button.disabled = false;
    mediaStatus.textContent =
      "The media could not be queued. Check the exact audio selection, native host and media tools. Retry the same selection or inspect a fresh source.";
  }
});
