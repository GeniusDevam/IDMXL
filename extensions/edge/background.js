const HOST = "dev.idmxl.host";
const MENU = "idmxl-send-link";
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => chrome.contextMenus.create({
    id: MENU, title: "Queue link in IDMXL", contexts: ["link"], targetUrlPatterns: ["http://*/*", "https://*/*"]
  }));
});

// Only the browser's context-menu event can create work. No content-script or web-message bridge.
chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== MENU || !info.linkUrl) return;
  let source;
  try { source = new URL(info.linkUrl); } catch { return; }
  if (!["http:", "https:"].includes(source.protocol) || source.username || source.password) return;
  let filename = "download.bin";
  try { filename = decodeURIComponent(source.pathname.split("/").pop()) || filename; } catch {}
  const request = {version:1,kind:"enqueue",request_id:crypto.randomUUID(),url:source.href,filename};
  try {
    const reply = await chrome.runtime.sendNativeMessage(HOST, request);
    if (reply.version !== 1 || reply.accepted !== true || reply.request_id !== request.request_id || !reply.job_id)
      throw new Error("rejected");
    await chrome.storage.local.set({lastStatus:"Queued in IDMXL. It can start if your queue is running; otherwise open the app to start it."});
    await chrome.action.setBadgeText({text:"✓"});
    await chrome.action.setBadgeBackgroundColor({color:"#20785c"});
  } catch {
    // Keep secrets out of storage/logs. The original browser action remains available.
    await chrome.storage.local.set({lastStatus:"Handoff failed. Check the native host setup, or paste the link into IDMXL."});
    await chrome.action.setBadgeText({text:"!"});
    await chrome.action.setBadgeBackgroundColor({color:"#ac4038"});
  }
});
