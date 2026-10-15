chrome.storage.local.get("lastStatus").then(({lastStatus}) => {
  if (lastStatus) document.getElementById("status").textContent = lastStatus;
});
