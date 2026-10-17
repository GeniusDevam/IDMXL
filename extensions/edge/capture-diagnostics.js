const reasons = {
  captured: "Moved to IDMXL. Its download queue started automatically; saved schedules and limits still apply.",
  captured_manual_start: "Moved to IDMXL, but automatic startup was not confirmed. Open the app and start its queue; update the installed app if necessary.",
  request_not_observed: "Kept in Chrome: the browser did not expose matching request details.",
  strong_identity_missing: "Kept in Chrome: the server did not provide the file identity required for a verified handoff.",
  browser_session_or_range: "Kept in Chrome: the request uses browser-session data or an existing partial transfer.",
  response_identity_unverified: "Kept in Chrome: the file size, identity or encoding could not be verified.",
  ambiguous_request: "Kept in Chrome: multiple matching requests made takeover ambiguous.",
  browser_only_source: "Kept in Chrome: this source cannot be transferred to the download engine.",
  browser_safety_check: "Kept in Chrome: the browser has not marked the download safe.",
  waiting_for_download_details: "Waiting for the browser to provide the file name and size, or the download is paused.",
  below_size_limit: "Kept in Chrome: the file is not larger than 1 MB.",
  capture_permission_missing: "Kept in Chrome: automatic capture is not allowed for this download.",
  browser_finished_first: "Chrome completed this file before a verified takeover could finish.",
  native_takeover_failed: "Kept in Chrome: IDMXL could not prepare a verified public download. The server may need your browser login, lack range support, or the native connection may be unavailable.",
};

export function captureReason(code) {
  return reasons[code] || "No download has been evaluated by this capture worker yet.";
}
