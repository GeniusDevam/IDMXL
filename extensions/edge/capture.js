// No request bodies, credential values or source URLs are persisted by automatic capture.
import { CAPTURE_PERMISSIONS } from "./capture-settings.js";
import { captureReason } from "./capture-diagnostics.js";
import { createBrowserBridge } from "./browser-bridge.js";

const HOST = "dev.idmxl.host";
export const MINIMUM_CAPTURE_BYTES = 1_000_000;
const terminal = new Set(["committed", "released"]);
function networkUrl(value) {
  const url = new URL(value);
  url.hash = "";
  return url.href;
}
function eligibleSource(download) {
  try {
    const source = new URL(download.finalUrl);
    const original = new URL(download.url);
    if (
      !["http:", "https:"].includes(source.protocol) ||
      !["http:", "https:"].includes(original.protocol) ||
      source.username ||
      source.password ||
      original.username ||
      original.password
    )
      return null;
    source.hash = "";
    return source;
  } catch {
    return null;
  }
}
function inScope(config, source) {
  return (
    config?.enabled &&
    (config.scope === "all" ||
      (config.rules || []).some(
        (r) =>
          r.origin === source.origin &&
          typeof r.prefix === "string" &&
          r.prefix.startsWith("/") &&
          r.prefix.endsWith("/") &&
          source.pathname.startsWith(r.prefix),
      ))
  );
}
function browserEligible(download) {
  return (
    !download.incognito &&
    !download.byExtensionId &&
    download.state === "in_progress" &&
    download.danger === "safe" &&
    Number.isSafeInteger(download.totalBytes) &&
    download.totalBytes > MINIMUM_CAPTURE_BYTES
  );
}
export function createCapture(
  api,
  clock = Date.now,
  uuid = () => crypto.randomUUID(),
  browser = null,
) {
  const requests = new Map();
  const pending = new Map();
  let stopped = false;
  let serial = Promise.resolve();
  const browserTimers = new Map();
  const enqueue = (fn) => (serial = serial.catch(() => {}).then(fn));
  async function status(message) {
    await api.storage.local.set({ lastStatus: message });
  }
  async function outcome(download, code, phase = "browser") {
    await api.storage.local.set({
      lastStatus: captureReason(code),
      lastCaptureAttempt: {
        code, phase, at: clock(),
        download_id: download.id,
        bytes: Number.isSafeInteger(download.totalBytes) ? download.totalBytes : -1,
      },
    });
    if (browser && phase === "browser") scheduleBrowser(download, code);
  }
  function scheduleBrowser(download, code) {
    if (stopped || browserTimers.has(download.id)) return;
    const delay = ["waiting_for_download_details", "request_not_observed"].includes(code) ? 750 : 0;
    const timer = setTimeout(() => {
      browserTimers.delete(download.id);
      void enqueue(async () => {
        if (stopped) return;
        const [current] = await api.downloads.search({id:download.id});
        if (!current || current.startTime !== download.startTime || current.incognito) return;
        const saved = (await records())[`${current.id}:${current.startTime}`];
        if (saved && saved.phase !== "released") return;
        await browser.track(current);
      }).catch(() => {});
    }, delay);
    timer?.unref?.();
    browserTimers.set(download.id,timer);
  }
  async function allowed(config, source) {
    return inScope(config, source) &&
      (config.scope !== "all" || await api.permissions.contains(CAPTURE_PERMISSIONS));
  }
  async function records() {
    return (await api.storage.local.get("captureRecords")).captureRecords || {};
  }
  async function save(record) {
    const all = await records();
    all[record.key] = record;
    const finished = Object.values(all)
      .filter((r) => terminal.has(r.phase))
      .sort((a, b) => a.updated - b.updated);
    while (Object.keys(all).length > 256 && finished.length)
      delete all[finished.shift().key];
    if (Object.keys(all).length > 256) throw new Error("handoff_capacity");
    await api.storage.local.set({ captureRecords: all });
  }
  async function setPhase(record, phase) {
    record.phase = phase;
    record.updated = clock();
    await save(record);
  }
  async function native(record, kind, fields = {}) {
    const reply = await api.runtime.sendNativeMessage(HOST, {
      version: 2,
      kind,
      request_id: record.request_id,
      ...fields,
    });
    if (
      reply?.version !== 2 ||
      reply.accepted !== true ||
      reply.request_id !== record.request_id ||
      !reply.job_id
    )
      throw new Error("native_rejected");
    return reply;
  }
  async function item(record) {
    const [download] = await api.downloads.search({ id: record.download_id });
    if (!download || download.startTime !== record.start_time)
      throw new Error("browser_item_missing");
    return download;
  }
  async function release(record) {
    try {
      await native(record, "abort");
    } catch {
      /* A held native job cannot run. */
    }
    const download = await item(record);
    if (
      download.state === "in_progress" &&
      download.paused &&
      record.paused_by_us
    ) {
      await api.downloads.resume(download.id);
      if ((await item(record)).paused) throw new Error("resume_unconfirmed");
    }
    await setPhase(record, "released");
    await status(
      "Download kept in the browser. IDMXL could not safely take it over.",
    );
    await outcome(download, "native_takeover_failed");
  }
  async function finish(record) {
    const download = await item(record);
    if (download.state !== "interrupted" || download.error !== "USER_CANCELED")
      throw new Error("cancellation_unconfirmed");
    let ready = await native(record, "status");
    if (ready.phase === "expired")
      ready = await native(record, "prepare", {
        url: eligibleSource(download)?.href,
        filename: record.filename,
        etag: record.etag,
        total: record.total,
      });
    if (!["ready", "committed"].includes(ready.phase))
      throw new Error("native_not_ready");
    const reply = await native(record, "commit", { browser_cancelled: true });
    if (reply.phase !== "committed") throw new Error("commit_unconfirmed");
    record.job_id = reply.job_id;
    await setPhase(record, "committed");
    const automatic = reply.activation?.queue_started === true;
    await outcome(download, automatic ? "captured" : "captured_manual_start", "idmxl");
    // Confirmation failure must never undo a committed handoff or recapture its job.
    try {
      await api.storage.local.set({
        captureConfirmation: { filename: record.filename, bytes: Number(record.total), job_id: record.job_id, at: clock(), automatic, desktop_requested: reply.activation?.desktop_requested === true },
      });
      await showCaptureConfirmation(api);
      await api.storage.local.set({ captureNoticeError: null });
    } catch {
      await api.storage.local.set({ captureNoticeError: "confirmation_unavailable" });
    }
  }
  async function recoverOne(record) {
    if (terminal.has(record.phase)) return;
    try {
      const download = await item(record);
      if (
        ["cancel_requested", "attention"].includes(record.phase) &&
        download.state === "interrupted" &&
        download.error === "USER_CANCELED"
      )
        await finish(record);
      else await release(record);
    } catch {
      await setPhase(record, "attention");
      await status(
        "A browser handoff needs attention. Check IDMXL and the browser's Downloads page. If the link expired, obtain a fresh link.",
      );
    }
  }
  function before(details) {
    if (stopped) return;
    if (
      details.incognito ||
      details.method !== "GET" ||
      details.url.length > 16384
    ) {
      requests.delete(details.requestId);
      return;
    }
    // Cookies are never copied. A public representation may still qualify if
    // native preparation independently verifies it without the browser session.
    // Authorization, partial transfers and incomplete observation stay in Chrome.
    const credentialed =
      !Array.isArray(details.requestHeaders) ||
      (details.requestHeaders || []).some((h) =>
        ["authorization", "proxy-authorization", "range"].includes(
          h.name.toLowerCase(),
        ),
      );
    requests.set(details.requestId, {
      url: networkUrl(details.url),
      at: clock(),
      credentialed,
    });
    if (requests.size > 256) requests.delete(requests.keys().next().value);
  }
  function headers(details) {
    if (stopped) return;
    const request = requests.get(details.requestId);
    if (!request || request.url !== networkUrl(details.url) || details.statusCode !== 200) {
      requests.delete(details.requestId);
      return;
    }
    const values = (name) =>
      (details.responseHeaders || [])
        .filter((h) => h.name.toLowerCase() === name)
        .map((h) => h.value);
    const [etag] = values("etag"),
      [length] = values("content-length");
    const encoding = values("content-encoding");
    request.valid =
      !request.credentialed &&
      values("etag").length === 1 &&
      /^"[^"\r\n]*"$/.test(etag || "") &&
      (etag || "").length <= 1024 &&
      values("content-length").length === 1 &&
      /^\d{1,16}$/.test(length || "") &&
      BigInt(length) > 0n &&
      BigInt(length) <= BigInt(Number.MAX_SAFE_INTEGER) &&
      (encoding.length === 0 ||
        (encoding.length === 1 &&
          typeof encoding[0] === "string" &&
          encoding[0].toLowerCase() === "identity"));
    request.etag = etag;
    request.total = request.valid ? BigInt(length).toString() : null;
    request.reason = request.credentialed ? "browser_session_or_range"
      : values("etag").length !== 1 || !/^"[^"\r\n]*"$/.test(etag || "") ? "strong_identity_missing"
      : !request.valid ? "response_identity_unverified" : "observed";
    request.at = clock();
    if (request.valid) void enqueue(async () => {
      for (const [id, waiting] of pending) {
        if (clock() - waiting.at >= 30000) { pending.delete(id); continue; }
        if (waiting.url !== request.url) continue;
        const [download] = await api.downloads.search({ id });
        if (download && download.startTime === waiting.startTime) await created(download);
      }
    }).catch(() => {});
  }
  async function created(download) {
    if (stopped) return;
    const config = (await api.storage.local.get("captureConfig")).captureConfig;
    if (!config?.enabled || download.incognito) return;
    if (browser && await browser.owns(download)) { await browser.update(download); return; }
    const saved = (await records())[`${download.id}:${download.startTime}`];
    if (browser && saved?.phase === "released") {
      scheduleBrowser(download, "native_takeover_failed");
      return;
    }
    if (download.state !== "in_progress") {
      pending.delete(download.id);
      if (download.state === "complete" && !(await records())[`${download.id}:${download.startTime}`])
        await outcome(download, "browser_finished_first");
      return;
    }
    const source = eligibleSource(download);
    if (!source) { await outcome(download, "browser_only_source"); return; }
    if (!(await allowed(config, source))) { await outcome(download, "capture_permission_missing"); return; }
    pending.set(download.id, { url: source.href, startTime: download.startTime, at: clock() });
    if (pending.size > 256) pending.delete(pending.keys().next().value);
    if (download.totalBytes <= MINIMUM_CAPTURE_BYTES && download.totalBytes >= 0) {
      await outcome(download, "below_size_limit"); return;
    }
    if (!browserEligible(download) || download.paused || !download.filename) {
      await outcome(download, download.danger !== "safe" ? "browser_safety_check" : "waiting_for_download_details"); return;
    }
    const observations = [...requests.values()].filter((r) => r.url === source.href && clock() - r.at < 30000);
    const matching = observations.filter(
      (r) =>
        r.valid &&
        r.total === String(download.totalBytes),
    );
    // A navigation and its download request can expose the same representation
    // twice. Collapse identical identities, but never guess between conflicting
    // or credential-bearing observations of the same URL.
    const candidates = [...new Map(matching.map((r) => [JSON.stringify([r.etag, r.total]), r])).values()];
    const ambiguous = observations.length > 1 && observations.some((r) => !r.valid);
    if (candidates.length !== 1 || ambiguous) {
      await outcome(download, !observations.length ? "request_not_observed" : candidates.length > 1 || ambiguous ? "ambiguous_request" : observations[0].reason === "observed" ? "response_identity_unverified" : observations[0].reason || "response_identity_unverified");
      return;
    }
    const key = `${download.id}:${download.startTime}`;
    if ((await records())[key]) return;
    pending.delete(download.id);
    const record = {
      key,
      request_id: uuid(),
      download_id: download.id,
      start_time: download.startTime,
      etag: candidates[0].etag,
      total: candidates[0].total,
      filename: download.filename.split(/[\\/]/).pop() || "download.bin",
      phase: "pausing",
      paused_by_us: true,
      updated: clock(),
    };
    // Journal before any browser mutation. Native preparation stays invisible to the scheduler.
    await save(record);
    try {
      await api.downloads.pause(download.id);
      const paused = await item(record);
      if (
        !browserEligible(paused) ||
        !paused.paused ||
        eligibleSource(paused)?.href !== source.href ||
        String(paused.totalBytes) !== record.total
      ) {
        await release(record);
        return;
      }
      const ready = await native(record, "prepare", {
        url: source.href,
        filename: record.filename,
        etag: record.etag,
        total: record.total,
      });
      if (ready.phase !== "ready" || ready.expires < clock() + 5000) {
        await release(record);
        return;
      }
      await setPhase(record, "prepared");
      const current = await item(record);
      const latest = (await api.storage.local.get("captureConfig")).captureConfig;
      if (
        !(await allowed(latest, source)) ||
        !browserEligible(current) ||
        !current.paused ||
        eligibleSource(current)?.href !== source.href ||
        String(current.totalBytes) !== record.total
      ) {
        await release(record);
        return;
      }
      await setPhase(record, "cancel_requested");
      await api.downloads.cancel(download.id);
      await finish(record);
    } catch {
      await recoverOne(record);
    }
  }
  return {
    before,
    headers,
    redirect: (details) => requests.delete(details.requestId),
    failed: (details) => {
      const observed = requests.get(details.requestId);
      // Chromium can detach a validated navigation response into its download manager.
      // An aborted navigation is not proof that the matching DownloadItem was canceled.
      if (details.error === "net::ERR_ABORTED" && observed?.valid) return;
      requests.delete(details.requestId);
    },
    idle: () => serial.catch(() => {}),
    stop: () => { stopped = true; requests.clear(); pending.clear(); for(const timer of browserTimers.values())clearTimeout(timer); browserTimers.clear(); },
    created: (download) => stopped ? Promise.resolve() : enqueue(() => created(download)),
    determiningFilename: (download, suggest) => {
      if (stopped || !browserEligible(download) || download.paused || !download.filename) {
        suggest();
        return false;
      }
      // Consider the tentative name early, without delaying Chrome's target
      // selection or renaming its file while asynchronous handoff checks run.
      void enqueue(() => created(download)).catch(() => {});
      suggest();
      return false;
    },
    changed: (delta) =>
      stopped ? Promise.resolve() : enqueue(async () => {
        if (
          !["totalBytes", "filename", "finalUrl", "danger", "state", "paused", "canResume", "exists", "error"].some(
            (key) => key in delta,
          )
        )
          return;
        const [download] = await api.downloads.search({ id: delta.id });
        if (download) await created(download);
      }),
    recover: () =>
      stopped ? Promise.resolve() : enqueue(async () => {
        for (const record of Object.values(await records()))
          await recoverOne(record);
        if (browser && !stopped) {
          const active = await api.downloads.search({state:"in_progress"});
          for (const download of active) await created(download);
        }
      }),
  };
}

export function attachCapture(api) {
  // Register synchronously when the worker starts so its first download event is not lost.
  if (!api.downloads || !api.webRequest) return null;
  const browser = createBrowserBridge(api, {showNotice:()=>showCaptureConfirmation(api)});
  const capture = createCapture(api, Date.now, () => crypto.randomUUID(), browser);
  const registered = [];
  const add = (event, listener, ...args) => {
    event.addListener(listener, ...args);
    registered.push([event, listener]);
  };
  try {
    const filter = { urls: ["http://*/*", "https://*/*"] };
    add(api.webRequest.onBeforeSendHeaders, capture.before, filter, [
      "requestHeaders",
      "extraHeaders",
    ]);
    add(api.webRequest.onHeadersReceived, capture.headers, filter, [
      "responseHeaders",
      "extraHeaders",
    ]);
    add(api.webRequest.onBeforeRedirect, capture.redirect, filter);
    add(api.webRequest.onErrorOccurred, capture.failed, filter);
    add(api.downloads.onCreated, capture.created);
    add(api.downloads.onChanged, capture.changed);
    add(api.downloads.onDeterminingFilename, capture.determiningFilename);
    add(api.runtime.onStartup, capture.recover);
  } catch {
    browser.dispose();
    for (const [event, listener] of registered) event.removeListener(listener);
    return null;
  }
  // A storage/recovery failure must not cause a second set of listeners to be installed.
  return {
    ready: browser.ready.then(() => capture.recover()),
    idle: () => Promise.all([capture.idle(),browser.idle()]),
    dispose: () => {
      capture.stop();
      browser.dispose();
      for (const [event, listener] of registered) event.removeListener(listener);
    },
    healthy: () => registered.every(([event, listener]) => !event.hasListener || event.hasListener(listener)),
    listenerCount: registered.length,
  };
}

export function installCapture(api) {
  const attached = attachCapture(api);
  return attached ? attached.ready.then(() => true, () => true) : Promise.resolve(false);
}

export async function showCaptureConfirmation(api) {
  if (!api.windows?.create) throw new Error("confirmation_unavailable");
  const { captureNoticeWindow } = api.storage.session
    ? await api.storage.session.get("captureNoticeWindow") : {};
  if (Number.isInteger(captureNoticeWindow)) {
    try { await api.windows.update(captureNoticeWindow, { focused: true }); return; } catch {}
  }
  const window = await api.windows.create({ url: api.runtime.getURL("capture-status.html"), type: "popup", width: 450, height: 400, focused: true });
  if (api.storage.session && Number.isInteger(window?.id))
    await api.storage.session.set({ captureNoticeWindow: window.id });
}
