// No request bodies, header values or source URLs are persisted by automatic capture.
const HOST = "dev.idmxl.host";
const terminal = new Set(["committed", "released"]);
export function createCapture(
  api,
  clock = Date.now,
  uuid = () => crypto.randomUUID(),
) {
  const requests = new Map();
  let serial = Promise.resolve();
  const enqueue = (fn) => (serial = serial.catch(() => {}).then(fn));
  async function status(message) {
    await api.storage.local.set({ lastStatus: message });
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
  }
  async function finish(record) {
    const download = await item(record);
    if (download.state !== "interrupted" || download.error !== "USER_CANCELED")
      throw new Error("cancellation_unconfirmed");
    let ready = await native(record, "status");
    if (ready.phase === "expired")
      ready = await native(record, "prepare", {
        url: download.finalUrl,
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
    await status(
      "Browser download moved to IDMXL. Open IDMXL to start it, or leave its queue running.",
    );
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
    if (
      details.incognito ||
      details.method !== "GET" ||
      details.url.length > 16384
    ) {
      requests.delete(details.requestId);
      return;
    }
    // Keep no credential values, and fail closed if observation is incomplete.
    const credentialed =
      !Array.isArray(details.requestHeaders) ||
      (details.requestHeaders || []).some((h) =>
        ["cookie", "authorization", "proxy-authorization", "range"].includes(
          h.name.toLowerCase(),
        ),
      );
    requests.set(details.requestId, {
      url: details.url,
      at: clock(),
      credentialed,
    });
    if (requests.size > 256) requests.delete(requests.keys().next().value);
  }
  function headers(details) {
    const request = requests.get(details.requestId);
    if (!request || request.url !== details.url || details.statusCode !== 200) {
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
      values("set-cookie").length === 0 &&
      (encoding.length === 0 ||
        (encoding.length === 1 && encoding[0].toLowerCase() === "identity"));
    request.etag = etag;
    request.total = length;
  }
  async function created(download) {
    const config = (await api.storage.local.get("captureConfig")).captureConfig;
    if (
      !config?.enabled ||
      download.incognito ||
      download.byExtensionId ||
      download.state !== "in_progress" ||
      download.paused ||
      download.danger !== "safe"
    )
      return;
    let source;
    try {
      source = new URL(download.finalUrl);
    } catch {
      return;
    }
    if (
      source.protocol !== "https:" ||
      source.username ||
      source.password ||
      source.search ||
      source.hash ||
      download.url !== download.finalUrl
    )
      return;
    if (
      !(config.rules || []).some(
        (r) =>
          r.origin === source.origin &&
          typeof r.prefix === "string" &&
          r.prefix.startsWith("/") &&
          r.prefix.endsWith("/") &&
          source.pathname.startsWith(r.prefix),
      )
    )
      return;
    const candidates = [...requests.values()].filter(
      (r) =>
        r.url === source.href &&
        r.valid &&
        clock() - r.at < 30000 &&
        r.total === String(download.totalBytes),
    );
    if (candidates.length !== 1 || download.totalBytes < 1048576) return;
    const key = `${download.id}:${download.startTime}`;
    if ((await records())[key]) return;
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
      if (paused.state !== "in_progress" || !paused.paused) {
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
      if (current.state !== "in_progress" || !current.paused) {
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
    failed: (details) => requests.delete(details.requestId),
    created: (download) => enqueue(() => created(download)),
    recover: () =>
      enqueue(async () => {
        for (const record of Object.values(await records()))
          await recoverOne(record);
      }),
  };
}

export async function installCapture(api) {
  const enabled = await api.permissions.contains({
    permissions: ["downloads", "webRequest"],
  });
  if (!enabled) return false;
  const capture = createCapture(api);
  api.webRequest.onBeforeSendHeaders.addListener(
    capture.before,
    { urls: ["https://*/*"] },
    ["requestHeaders", "extraHeaders"],
  );
  api.webRequest.onHeadersReceived.addListener(
    capture.headers,
    { urls: ["https://*/*"] },
    ["responseHeaders", "extraHeaders"],
  );
  api.webRequest.onBeforeRedirect.addListener(capture.redirect, {
    urls: ["https://*/*"],
  });
  api.webRequest.onErrorOccurred.addListener(capture.failed, {
    urls: ["https://*/*"],
  });
  api.downloads.onCreated.addListener(capture.created);
  api.runtime.onStartup.addListener(capture.recover);
  await capture.recover();
  return true;
}
