@echo off
setlocal
set "app=%~dp0apps\desktop\src-tauri\target\release\idmxl-desktop.exe"
if not exist "%app%" set "app=%~dp0apps\desktop\src-tauri\target\debug\idmxl-desktop.exe"
if not exist "%app%" (
  echo Build the desktop preview first with scripts\build.ps1.
  exit /b 1
)
start "" "%app%"
