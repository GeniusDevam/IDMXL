!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog 'powershell.exe -NoLogo -NoProfile -NonInteractive -File "$INSTDIR\register-torrents.ps1" -Executable "$INSTDIR\idmxl-desktop.exe"'
!macroend
