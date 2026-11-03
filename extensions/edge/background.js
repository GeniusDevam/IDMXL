import { createCaptureManager } from "./capture-manager.js";
const captureManager = createCaptureManager(chrome);
void captureManager.start().catch(() => {});
const HOST = "dev.idmxl.host";
const MENU = "idmxl-send-link";
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() =>
    chrome.contextMenus.create({
      id: MENU,
      title: "Queue link in IDMXL",
      contexts: ["link"],
    }),
  );
});

// Explicit links use the browser's context-menu event. There is no web-message bridge.
chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== MENU || !info.linkUrl) return;
  let source;
  try {
    source = new URL(info.linkUrl);
  } catch {
    return;
  }
  if (source.protocol === "magnet:" || /\.torrent$/i.test(source.pathname)) { await queueTorrent(source.href); return; }
  if (
    !["http:", "https:"].includes(source.protocol) ||
    source.username ||
    source.password
  )
    return;
  let filename = "download.bin";
  try {
    filename = decodeURIComponent(source.pathname.split("/").pop()) || filename;
  } catch {}
  const request = {
    version: 1,
    kind: "enqueue",
    request_id: crypto.randomUUID(),
    url: source.href,
    filename,
    confirm_destination:true,
  };
  try {
    const reply = await chrome.runtime.sendNativeMessage(HOST, request);
    if (
      reply.version !== 1 ||
      reply.accepted !== true ||
      reply.request_id !== request.request_id ||
      !reply.job_id
    )
      throw new Error("rejected");
    await chrome.storage.local.set({
      lastStatus:
        "Queued in IDMXL. It can start if your queue is running; otherwise open the app to start it.",
    });
    await chrome.action.setBadgeText({ text: "✓" });
    await chrome.action.setBadgeBackgroundColor({ color: "#20785c" });
  } catch {
    // Keep secrets out of storage/logs. The original browser action remains available.
    await chrome.storage.local.set({
      lastStatus:
        "Handoff failed. Check the native host setup, or paste the link into IDMXL.",
    });
    await chrome.action.setBadgeText({ text: "!" });
    await chrome.action.setBadgeBackgroundColor({ color: "#ac4038" });
  }
});

async function queueTorrent(url) {
  const request_id = crypto.randomUUID();
  try {
    const reply = await chrome.runtime.sendNativeMessage(HOST, {version:7,kind:"torrent_inspect",request_id,url});
    if (!reply?.accepted || reply.version!==7 || reply.request_id!==request_id || !reply.job_id) throw new Error();
    await chrome.storage.local.set({lastStatus:"Choose the torrent files and download folder in IDMXL."});
  } catch { await chrome.storage.local.set({lastStatus:"Torrent handoff failed. Open IDMXL and paste the magnet link or torrent URL."}); }
}
chrome.runtime.onMessage?.addListener((message,sender,reply)=>{
  if(sender?.id!==chrome.runtime.id || !sender.tab || message?.kind!=="torrent-link" || typeof message.url!=="string" || !message.url.startsWith("magnet:?"))return false;
  void queueTorrent(message.url).then(()=>reply({accepted:true}),()=>reply({accepted:false})); return true;
});
async function registerTorrentClicks() {
  if (!chrome.scripting?.registerContentScripts || !chrome.permissions?.contains) return;
  if (!(await chrome.permissions.contains({origins:["http://*/*","https://*/*"]}))) return;
  try { await chrome.scripting.registerContentScripts([{id:"idmxl-torrent-links",js:["torrent-links.js"],matches:["http://*/*","https://*/*"],runAt:"document_start",allFrames:true,persistAcrossSessions:true}]); } catch {}
}
chrome.permissions?.onAdded?.addListener(()=>void registerTorrentClicks());
void registerTorrentClicks();
