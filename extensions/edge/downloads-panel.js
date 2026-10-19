const HOST = "dev.idmxl.host";
export async function requestDownloads(api, kind = "jobs", fields = {}) {
  const request_id = crypto.randomUUID();
  const reply = await api.runtime.sendNativeMessage(HOST, {
    version: 5,
    kind,
    request_id,
    ...fields,
  });
  if (
    reply?.version !== 5 ||
    !reply.accepted ||
    reply.request_id !== request_id ||
    !Array.isArray(reply.jobs) ||
    reply.jobs.length > 100
  )
    throw new Error("downloads_unavailable");
  return reply.jobs;
}
export function downloadButtons(job) {
  const active = ["downloading", "verifying", "pausing"].includes(job.state);
  const terminal = [
    "completed",
    "canceled",
    "publishing",
    "attention",
  ].includes(job.state);
  const buttons = [];
  if (active && job.state !== "pausing")
    buttons.push({ action: "pause", label: "Pause" });
  else if (!terminal && job.state !== "pausing" && job.can_resume !== false)
    buttons.push({
      action:
        job.state === "failed" && job.mode !== "browser" ? "retry" : "start",
      label:
        job.state === "failed" && job.mode !== "browser"
          ? "Retry now"
          : "Resume",
    });
  if (!["completed", "canceled", "publishing"].includes(job.state))
    buttons.push({ action: "cancel", label: "Cancel" });
  return buttons;
}
function size(value) {
  if (value === null || value === undefined) return "unknown size";
  const n = Number(value);
  return n < 1e6 ? `${(n / 1000).toFixed(1)} KB` : `${(n / 1e6).toFixed(1)} MB`;
}
export function startDownloadPanel(api, doc) {
  const list = doc.getElementById("live-downloads"),
    status = doc.getElementById("live-status");
  let stopped = false,
    busy = false,
    timer;
  async function refresh() {
    if (stopped || busy) return;
    busy = true;
    try {
      render(await requestDownloads(api));
      status.textContent = "Updated live from IDMXL.";
    } catch {
      status.textContent = "Connect the matching IDMXL app to show downloads.";
    } finally {
      busy = false;
      if (!stopped) timer = setTimeout(refresh, 1000);
    }
  }
  function render(jobs) {
    list.replaceChildren();
    if (!jobs.length) {
      const p = doc.createElement("p");
      p.textContent = "No ongoing downloads.";
      list.appendChild(p);
    }
    for (const job of jobs) {
      const row = doc.createElement("article"),
        name = doc.createElement("strong"),
        detail = doc.createElement("p"),
        progress = doc.createElement("progress"),
        actions = doc.createElement("div");
      name.textContent = String(job.name || "Download");
      detail.textContent = `${job.mode === "browser" ? "Browser transfer · " : ""}${job.state} · ${size(job.received_bytes)} / ${size(job.total_bytes)}`;
      progress.max = 100;
      if (Number(job.total_bytes) > 0)
        progress.value = Math.min(
          100,
          (Number(job.received_bytes) / Number(job.total_bytes)) * 100,
        );
      for (const { action, label } of downloadButtons(job)) {
        const button = doc.createElement("button");
        button.type = "button";
        button.textContent = label;
        button.addEventListener("click", async () => {
          if (busy) return;
          busy = true;
          clearTimeout(timer);
          for (const element of list.querySelectorAll("button"))
            element.disabled = true;
          try {
            render(
              await requestDownloads(api, "control", {
                job_id: job.id,
                action,
              }),
            );
            status.textContent = "Action sent to IDMXL.";
          } catch {
            status.textContent =
              "Could not apply that action. Check IDMXL and try again.";
          } finally {
            busy = false;
            if (!stopped) timer = setTimeout(refresh, 750);
          }
        });
        actions.appendChild(button);
      }
      row.append(name, detail, progress, actions);
      list.appendChild(row);
    }
  }
  void refresh();
  return () => {
    stopped = true;
    clearTimeout(timer);
  };
}
