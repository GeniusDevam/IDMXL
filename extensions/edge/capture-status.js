async function showReceipt() {
  const { captureConfirmation } = await chrome.storage.local.get("captureConfirmation");
  document.getElementById("filename").textContent = captureConfirmation?.filename || "Download";
  document.getElementById("size").textContent = Number.isFinite(captureConfirmation?.bytes)
    ? (captureConfirmation.bytes / 1_000_000).toFixed(2) + " MB" : "";
  document.getElementById("result").textContent = captureConfirmation?.job_id
    ? "This file was queued in IDMXL. Chrome's transfer was canceled after the verified handoff."
    : "No capture receipt is available. Check the extension and Chrome Downloads.";
  document.getElementById("startup").textContent = !captureConfirmation?.job_id ? ""
    : captureConfirmation.automatic
      ? (captureConfirmation.desktop_requested ? "IDMXL is opening. " : "Open IDMXL to see progress. ") + "Its queue started automatically. Saved schedules, limits and earlier jobs can affect when this file begins."
      : "Automatic startup was not confirmed. Open IDMXL and start its queue. If necessary, update the installed app.";
}
void showReceipt();
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "local" && changes.captureConfirmation) void showReceipt();
});
document.getElementById("close").addEventListener("click", () => window.close());
