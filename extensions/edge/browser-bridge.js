// Browser-owned downloads are never replayed, canceled or moved by registration.
const HOST = "dev.idmxl.host";
const safe = new Set(["safe", "accepted", "deepScannedSafe", "allowlistedByPolicy"]);
const key = (download) => `${download.id}:${download.startTime}`;
const count = (n) => Number.isSafeInteger(n) && n >= 0 ? n : 0;

export function browserReport(download, id, previous, now) {
  let origin = null;
  try {
    const source = new URL(download.finalUrl || download.url);
    if (/^https?:$/.test(new URL(source.origin).protocol)) origin = source.origin;
  } catch { /* Browser-only URLs have no transferable origin. */ }
  const state = download.state === "complete" ? "completed"
    : download.state === "interrupted" ? download.error === "USER_CANCELED" ? "canceled" : "failed"
    : !safe.has(download.danger) ? "attention" : download.paused ? "paused" : "downloading";
  const received = count(download.bytesReceived);
  const elapsed = previous ? now - previous.at : 0;
  return {
    id, name: (download.filename?.split(/[\\/]/).pop() || "Download").replace(/[\x00-\x1f\x7f]/g, "_").slice(0, 240),
    origin, state, total: Number.isSafeInteger(download.totalBytes) && download.totalBytes >= 0 ? download.totalBytes : null,
    received, speed: state === "downloading" && elapsed > 0 ? Math.max(0, Math.floor((received - previous.received) * 1000 / elapsed)) : 0,
    can_resume: download.canResume === true, exists: download.state === "complete" && download.exists === true,
  };
}

export function createBrowserBridge(api, options = {}) {
  const clock = options.clock || Date.now;
  const uuid = options.uuid || (() => crypto.randomUUID());
  const setTimer = options.setTimer || setTimeout;
  const clearTimer = options.clearTimer || clearTimeout;
  let records = {}, ledger = {}, client, stopped = false, timer = null;
  let serial = Promise.resolve();
  const reports = new Map(), snapshots = new Map();
  const enqueue = (fn) => serial = serial.catch(() => {}).then(fn);
  const ready = (async () => {
    const saved = await api.storage.local.get(["browserTransfers", "browserTransferClient", "browserCommandLedger"]);
    records = saved.browserTransfers || {};
    ledger = saved.browserCommandLedger || {};
    // An interrupted command is ambiguous. Do not repeat it after a worker crash.
    for (const entry of Object.values(ledger)) if (entry.pending) { entry.pending = false; entry.success = false; }
    client = saved.browserTransferClient || uuid();
    await api.storage.local.set({ browserTransferClient: client, browserCommandLedger: ledger });
  })();
  ready.catch(() => {});

  async function enabled() {
    const { captureConfig } = await api.storage.local.get("captureConfig");
    return !stopped && captureConfig?.enabled === true && captureConfig.scope === "all" && captureConfig.browserAssisted !== false;
  }
  async function persist() {
    await api.storage.local.set({ browserTransfers: records, browserCommandLedger: ledger });
  }
  function note(download, record) {
    const report = browserReport(download, record.request_id, snapshots.get(record.request_id), clock());
    snapshots.set(record.request_id, { received: report.received, at: clock() });
    reports.set(record.request_id, report);
    record.terminal = ["completed", "canceled"].includes(report.state) || report.state === "failed" && !report.can_resume;
    return report;
  }
  async function apply(command) {
    if (!command || !/^[0-9a-f-]{36}$/i.test(command.id || "") || !["pause", "resume", "cancel", "show", "delete"].includes(command.action)) return false;
    if (ledger[command.id]) return false;
    const record = Object.values(records).find(record => record.request_id === command.transfer_id);
    ledger[command.id] = { pending: true, success: false, at: clock() };
    await persist();
    let success = false;
    try {
      if (!record || clock() - command.created > 120_000) throw new Error("stale_command");
      const [download] = await api.downloads.search({ id: record.download_id });
      if (!download || download.startTime !== record.start_time || download.incognito) throw new Error("browser_item_missing");
      if (command.action === "pause") {
        if (download.state !== "in_progress") throw new Error("not_running");
        if (!download.paused) await api.downloads.pause(download.id);
      }
      else if (command.action === "resume") {
        if (!download.canResume || !safe.has(download.danger)) throw new Error("cannot_resume");
        await api.downloads.resume(download.id);
      } else if (command.action === "cancel") {
        if (download.state === "in_progress") await api.downloads.cancel(download.id);
        else if (download.error !== "USER_CANCELED") throw new Error("not_running");
      } else if (command.action === "delete") {
        if (download.state === "in_progress") await api.downloads.cancel(download.id);
        if (download.state === "complete" && download.exists) await api.downloads.removeFile(download.id);
      } else if (command.action === "show") {
        if (download.state !== "complete" || !download.exists) throw new Error("file_unavailable");
        await api.downloads.show(download.id);
      }
      success = true;
      const [current] = await api.downloads.search({ id: download.id });
      if (current?.startTime === record.start_time) note(command.action === "delete" ? {...current,exists:false} : current, record);
    } catch { /* Only a bounded outcome is sent back; never raw browser errors. */ }
    ledger[command.id] = { pending: false, success, at: clock() };
    const old = Object.entries(ledger).sort((a,b) => a[1].at-b[1].at);
    while (old.length > 256) delete ledger[old.shift()[0]];
    await persist();
    return true;
  }
  async function sync() {
    if (!await enabled()) return;
    try {
      // A command result may create one further progress report; chunk large batches.
      for (let batch = 0; batch < 32; batch++) {
        const sent = [...reports.values()].slice(0,64);
        const acknowledgements = Object.entries(ledger).filter(([,v]) => !v.pending).map(([id,v]) => ({id,success:v.success}));
        const requestId = uuid();
        const reply = await api.runtime.sendNativeMessage(HOST, { version:4, kind:"browser_sync", request_id:requestId, client_id:client, reports:sent, acknowledgements });
        if (reply?.version !== 4 || !reply.accepted || reply.request_id !== requestId || reply.client_id !== client || !Array.isArray(reply.commands) || reply.commands.length > 32) throw new Error("browser_bridge_unavailable");
        for (const report of sent) if (reports.get(report.id) === report) reports.delete(report.id);
        let announced = false;
        for (const report of sent) {
          const record = Object.values(records).find(r => r.request_id === report.id);
          if (!record || record.registered) continue;
          record.registered = true;
          if (!announced && options.showNotice && !["canceled","failed"].includes(report.state)) {
            announced = true;
            await api.storage.local.set({ captureConfirmation: { transport:"browser", filename:report.name, bytes:report.total, job_id:report.id, state:report.state, desktop_requested:reply.desktop_requested === true, at:clock() }, lastStatus:"Browser transfer added to IDMXL. Keep the browser open; it handles this file using its existing session.", lastCaptureAttempt:{code:"browser_transfer",phase:"browser_assisted",at:clock(),download_id:record.download_id,bytes:report.total ?? -1}, browserBridgeError:null });
            try { await options.showNotice(); } catch { /* Tracking must survive a missing notice window. */ }
          }
        }
        let applied = false;
        for (const command of reply.commands) applied = (await apply(command)) || applied;
        await persist();
        await api.storage.local.set({ browserBridgeError:null });
        if (!reports.size && !applied) break;
      }
    } catch {
      await api.storage.local.set({ browserBridgeError:"Browser transfer connection is unavailable. Keep the download in Chrome and update/check the IDMXL app connection." });
    }
  }
  function schedule() {
    if (stopped || timer !== null || !Object.keys(records).length || options.autoPoll === false) return;
    timer = setTimer(() => {
      timer = null;
      void enqueue(async () => { await poll(); }).finally(schedule).catch(() => {});
    }, 2000);
    timer?.unref?.();
  }
  async function poll() {
    await ready;
    if (!await enabled()) return;
    for (const record of Object.values(records)) {
      if (record.terminal) continue;
      const [download] = await api.downloads.search({ id:record.download_id });
      if (download && download.startTime === record.start_time && !download.incognito) note(download, record);
    }
    await sync();
  }
  void ready.then(schedule, () => {});
  return {
    ready,
    owns: async download => { await ready; return Boolean(records[key(download)]); },
    track: download => enqueue(async () => {
      await ready;
      if (!await enabled() || download.incognito) return false;
      const identity = key(download);
      let record = records[identity];
      if (!record) {
        if (Object.keys(records).length >= 1000) {
          const old = Object.entries(records).find(([,value]) => value.terminal);
          if (!old) return false;
          delete records[old[0]];
        }
        record = records[identity] = { download_id:download.id, start_time:download.startTime, request_id:uuid(), terminal:false, registered:false };
      }
      note(download,record); await persist(); await sync(); schedule(); return true;
    }),
    update: download => enqueue(async () => {
      await ready;
      const record = records[key(download)];
      if (!record || stopped) return;
      note(download,record); await sync(); schedule();
    }),
    flush: () => enqueue(poll),
    idle: () => serial.catch(() => {}),
    dispose: () => { stopped=true; if(timer!==null)clearTimer(timer); timer=null; },
  };
}
