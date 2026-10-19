export const CAPTURE_PERMISSIONS = {
  permissions: ["downloads", "webRequest"],
  origins: ["http://*/*", "https://*/*"],
};

export async function enableAutomaticCapture(api) {
  // This request must run directly from the user's button click.
  if (!(await api.permissions.request(CAPTURE_PERMISSIONS))) return false;
  await api.storage.local.set({
    captureConfig: { enabled: true, scope: "all", browserAssisted: true },
  });
  const state = await api.runtime.sendMessage({ kind: "capture-refresh" });
  if (!state?.ready) throw new Error("capture_worker_unavailable");
  return true;
}

export async function disableAutomaticCapture(api) {
  await api.storage.local.set({
    captureConfig: { enabled: false, scope: "all" },
  });
  await api.runtime.sendMessage({ kind: "capture-refresh" });
}
