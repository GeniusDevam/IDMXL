# Download locations and controls

Extension **0.1.5** needs its matching updated IDMXL application/native host.

## Choose where to save

Native automatic captures, explicit extension links and extension media handoffs open **IDMXL — Choose download location** before a job can start. Choose the filename/folder and select Save. **Cancel or closing this dialog cancels the browser download**; it does not resume it in Chrome. Repeated host acknowledgements reuse the existing choice. An existing file is never silently overwritten; choose another name.

The app's New download and Download media forms also review the destination before adding anything. Canceling or closing those forms adds no download. Link import reviews one chosen folder and any category rules for the batch.

A folder becomes the suggested native default after **four distinct confirmed uses**. Replaying the same confirmation does not increment its count. **Change default folder** in the app sets the suggestion explicitly. Each native download still asks for confirmation. Extension save choices and the desktop use the same profile setting.

Select a queued, paused or failed native file in the app and choose **Change download location**. Pause running native transfers first. Checkpointed data is copied before changing the saved destination; the old copy is removed only after the database update. A file at the new target is never replaced. Prepared media jobs retain their original workspace/destination; choose their output location when adding them. This control does not move already completed files.

**Browser transfers** use Chrome or Edge's original download and Save As dialog. Keep the browser's “Ask where to save each file” setting enabled if you want its chooser each time. Chrome's downloads API cannot force a new arbitrary absolute destination for an existing session-dependent download. Native folder learning and relocation therefore apply to native jobs. The default folder for future native downloads remains editable in IDMXL.

## Progress and per-file actions

Open the extension to see **Ongoing downloads**, updated from IDMXL while the popup is open. It shows names, progress, state and Pause, Resume, Cancel or **Retry now**, as appropriate. Transfers are listed after native ownership is committed; the destination is confirmed before that point. Browser transfers use the same browser item shown in the app.

**Retry now** gives the failed native job a fresh retry budget and overrides its saved server wait for one explicit attempt. It does not clear waits for unrelated jobs on the same server. A fresh server throttle response restores normal waiting. Existing partial-data and source-identity checks remain enforced. A running native job must finish or pause before another starts. Browser retries/resumes are limited by the browser's own resume capability.

A successfully committed native capture cancels Chrome's original transfer before starting the native job. Filename selection is held while ownership is decided to avoid a second Chrome Save As flow. If native verification fails, the original browser transfer may continue as **Browser transfer**. That is one browser-owned download. Browser session/security restrictions still apply.

## Delete files and act on all downloads

Select a download and choose **Delete file from PC**. A separate confirmation names the file and explains that deletion is permanent. **Confirm deletion** stops that download if needed and deletes its recorded final file or partial file. Its history entry stays visible. For completed native files, the recorded publication identity must still match the file being removed; replaced files and reparse points are rejected. Browser-file deletion uses the browser's matching download ID/start time and requires the browser connection.

The app also offers **Pause all**, **Cancel all** and **Delete all files**. Each opens its own confirmation with the number of affected downloads. The action applies to that snapshot; a later arrival is not silently added to a confirmed deletion. Cancel keeps native partial files. Delete stops selected transfers before removing their files. Downloads that are still publishing or cannot stop in time are preserved and reported as unsuccessful; retry the action after they settle. Browser controls are sent to the browser and can report a later connection/control failure.

**Cancel, the confirmation's × button and Escape close the confirmation without performing the action.** The confirmation initially focuses Cancel. No destructive action is attached to closing the window. Once Confirm is selected, wait for the operation to finish.

The native queue controls and bulk actions are separate. Browser transfers remain browser-owned and do not gain IDMXL's native acceleration, speed limits, scheduling or SHA-256 verification.

Browser constraints follow the official [Chrome downloads API](https://developer.chrome.com/docs/extensions/reference/api/downloads): filename suggestions for existing transfers are relative to the browser Downloads folder, and the browser owns session-dependent transfers.
