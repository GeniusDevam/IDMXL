// Only real user clicks can open a torrent; page messages are never trusted.
let idmxlCaptureEnabled = false;
chrome.storage.local.get("captureConfig").then(({captureConfig}) => {
  idmxlCaptureEnabled = captureConfig?.enabled === true && captureConfig.scope === "all";
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "local" && changes.captureConfig) {
    const config=changes.captureConfig.newValue;
    idmxlCaptureEnabled=config?.enabled===true && config.scope==="all";
  }
});
document.addEventListener("click", event => {
  if (!idmxlCaptureEnabled || !event.isTrusted || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
  const anchor = event.target.closest?.("a[href]");
  if (!anchor?.href?.startsWith("magnet:?")) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  chrome.runtime.sendMessage({kind:"torrent-link",url:anchor.href}).catch(()=>{});
}, true);
