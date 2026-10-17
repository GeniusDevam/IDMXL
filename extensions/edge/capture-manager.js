import { attachCapture } from "./capture.js";
import { CAPTURE_PERMISSIONS } from "./capture-settings.js";

export function createCaptureManager(api) {
  // Attach before yielding on a cold worker start. Permission refresh can replace
  // an early registration after optional APIs have become available.
  let attached = attachCapture(api);
  attached?.ready.catch(() => {});
  let serial = Promise.resolve();
  async function detach() {
    if (!attached) return;
    attached.dispose();
    await attached.idle();
    attached = null;
  }
  async function save(ready, enabled, reason) {
    const state = { ready, enabled, reason, listeners: ready ? attached.listenerCount : 0 };
    await api.storage.local.set({ captureRuntime: state });
    return state;
  }
  async function sync(force) {
    const { captureConfig } = await api.storage.local.get("captureConfig");
    const enabled = captureConfig?.enabled === true;
    const permissions = captureConfig?.scope === "all" ? CAPTURE_PERMISSIONS
      : { permissions: CAPTURE_PERMISSIONS.permissions };
    if (!enabled || !(await api.permissions.contains(permissions))) {
      await detach();
      return save(false, enabled, enabled ? "permission_missing" : "off");
    }
    if (force || !attached?.healthy()) await detach();
    if (!attached) attached = attachCapture(api);
    if (!attached) return save(false, true, "listeners_unavailable");
    try {
      await attached.ready;
      if (!attached.healthy()) { await detach(); return save(false, true, "listeners_unavailable"); }
      return save(true, true, "ready");
    } catch {
      await detach();
      return save(false, true, "recovery_failed");
    }
  }
  function refresh(force = false) {
    serial = serial.catch(() => {}).then(() => sync(force));
    return serial;
  }
  const permissionChanged = () => { void refresh(true).catch(() => {}); };
  api.permissions.onAdded.addListener(permissionChanged);
  api.permissions.onRemoved?.addListener(permissionChanged);
  api.runtime.onMessage.addListener((message, sender, reply) => {
    if (sender?.id !== api.runtime.id ||
        ![api.runtime.getURL("popup.html"), api.runtime.getURL("capture-status.html")].includes(sender.url) ||
        !["capture-state", "capture-refresh"].includes(message?.kind)) return false;
    void refresh(message.kind === "capture-refresh").then(reply, () =>
      reply({ ready: false, enabled: false, reason: "initialization_failed", listeners: 0 }));
    return true;
  });
  return { start: () => refresh(), refresh };
}
