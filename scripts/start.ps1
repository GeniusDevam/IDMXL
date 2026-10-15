param([switch]$Release)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$profile = if ($Release) { 'release' } else { 'debug' }
$executable = Join-Path $repo "apps\desktop\src-tauri\target\$profile\idmxl-desktop.exe"
if (-not (Test-Path -LiteralPath $executable)) { throw 'Build the desktop app first with scripts\build.ps1.' }
Start-Process -FilePath $executable -WindowStyle Hidden
