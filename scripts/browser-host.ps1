param(
    [ValidateSet('Prepare','Install','Uninstall')][string]$Action = 'Prepare',
    [ValidatePattern('^[a-p]{32}$')][string]$ExtensionId,
    [string]$DownloadDirectory,
    [string]$StateDirectory,
    [string]$Executable,
    [ValidateSet('Edge','Chrome')][string]$Browser = 'Edge'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $StateDirectory) { $StateDirectory = Join-Path $env:LOCALAPPDATA 'IDMXL' }
$registry = if ($Browser -eq 'Edge') { 'HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\dev.idmxl.host' } else { 'HKCU:\Software\Google\Chrome\NativeMessagingHosts\dev.idmxl.host' }
if ($Action -eq 'Uninstall') {
    if (Test-Path -LiteralPath $registry) { Remove-Item -LiteralPath $registry }
    Write-Output 'Removed only the selected browser registration; downloads and profile were preserved.'
    return
}
if (-not $ExtensionId -or -not $DownloadDirectory) { throw 'Prepare/Install require ExtensionId and DownloadDirectory' }
if (-not $Executable) {
    $installedEngine = Join-Path $PSScriptRoot 'idmxl.exe'
    $Executable = if (Test-Path -LiteralPath $installedEngine) { $installedEngine } else { Join-Path $repo 'target\debug\idmxl.exe' }
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$DownloadDirectory = (Resolve-Path -LiteralPath $DownloadDirectory).Path
if (-not (Test-Path -LiteralPath $DownloadDirectory -PathType Container)) { throw 'Download directory must exist' }
if ($Action -eq 'Install' -and [IO.Path]::GetFullPath($StateDirectory) -ne [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'IDMXL'))) { throw 'Install requires the default state directory; Prepare supports isolated test profiles.' }
New-Item -ItemType Directory -Path $StateDirectory -Force | Out-Null
$StateDirectory = (Resolve-Path -LiteralPath $StateDirectory).Path
$origin = "chrome-extension://$ExtensionId/"
$manifest = Join-Path $StateDirectory 'dev.idmxl.host.json'
$utf8 = New-Object System.Text.UTF8Encoding($false)
[IO.File]::WriteAllText($manifest,(@{name='dev.idmxl.host';description='IDMXL explicit file-link handoff';path=$Executable;type='stdio';allowed_origins=@($origin)} | ConvertTo-Json -Depth 4),$utf8)
[IO.File]::WriteAllText((Join-Path $StateDirectory 'browser-host.json'),(@{allowed_origin=$origin;state_dir=$StateDirectory;download_dir=$DownloadDirectory} | ConvertTo-Json),$utf8)
if ($Action -eq 'Install') {
    # Chromium launches the host without custom arguments, using the default profile config.
    New-Item -Path $registry -Force | Out-Null
    Set-Item -LiteralPath $registry -Value $manifest
}
Write-Output "Native host files prepared: $manifest"
Write-Output "Registry action: $Action ($Browser). Load extensions\edge unpacked and use its exact extension ID."
