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
  document.getElementById("rules").textContent = captureConfig?.enabled
    ? "Enabled: " +
      captureConfig.rules.map((r) => r.origin + r.prefix).join(", ")
    : "Capture is off.";
}
void showRules();
document
  .getElementById("capture-form")
  .addEventListener("submit", async (event) => {
    event.preventDefault();
    const status = document.getElementById("status");
    try {
      const url = new URL(document.getElementById("folder").value);
      if (
        url.protocol !== "https:" ||
        url.username ||
        url.password ||
        url.search ||
        url.hash ||
        !url.pathname.endsWith("/")
      )
        throw new Error();
      const granted = await chrome.permissions.request({
        permissions: ["downloads", "webRequest"],
        origins: [url.origin + "/*"],
      });
      if (!granted) {
        status.textContent = "Capture permission was not granted.";
        return;
      }
      const { captureConfig } = await chrome.storage.local.get("captureConfig");
      const rules = (captureConfig?.rules || [])
        .filter((r) => r.origin !== url.origin || r.prefix !== url.pathname)
        .slice(-19);
      rules.push({ origin: url.origin, prefix: url.pathname });
      await chrome.storage.local.set({
        captureConfig: { enabled: true, rules },
      });
      status.textContent = "Public-file capture is enabled for this folder.";
      await showRules();
    } catch {
      status.textContent =
        "Use an HTTPS folder address ending in /, without a query or login details.";
    }
  });
document.getElementById("disable").addEventListener("click", async () => {
  await chrome.storage.local.set({
    captureConfig: { enabled: false, rules: [] },
  });
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
