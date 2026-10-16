param([switch]$Release, [switch]$Bundle)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$frontend = Join-Path $repo 'apps\desktop'
$native = Join-Path $frontend 'src-tauri'
$profile = if ($Release -or $Bundle) { 'release' } else { 'debug' }
. (Join-Path $PSScriptRoot 'tools.ps1')
Initialize-IDMXLTools

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}

$cargo = (Get-Command cargo.exe -ErrorAction Stop).Source
$npm = (Get-Command npm.cmd -ErrorAction Stop).Source
$cargoArgs = @('build', '--locked', '--manifest-path', (Join-Path $repo 'Cargo.toml'))
if ($profile -eq 'release') { $cargoArgs += '--release' }
Invoke-Checked -Program $cargo -Arguments $cargoArgs
if (-not (Test-Path -LiteralPath (Join-Path $frontend 'node_modules'))) {
    Invoke-Checked -Program $npm -Arguments @('--prefix', $frontend, 'ci', '--no-audit', '--no-fund')
}
Invoke-Checked -Program $npm -Arguments @('--prefix', $frontend, 'run', 'build')
$binaries = Join-Path $native 'binaries'
New-Item -ItemType Directory -Path $binaries -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $repo "target\$profile\idmxl.exe") -Destination (Join-Path $binaries 'idmxl-x86_64-pc-windows-msvc.exe') -Force
if ($Bundle) {
    Push-Location $frontend
    try { Invoke-Checked -Program $npm -Arguments @('run', 'tauri', '--', 'build') }
    finally { Pop-Location }
} else {
    $nativeArgs = @('build','--locked','--features','custom-protocol','--manifest-path',(Join-Path $native 'Cargo.toml'))
    if ($profile -eq 'release') { $nativeArgs += '--release' }
    Invoke-Checked -Program $cargo -Arguments $nativeArgs
}
# Keep the checkout launcher paired with the newly built engine after bundle builds too.
Copy-Item -LiteralPath (Join-Path $repo "target\$profile\idmxl.exe") -Destination (Join-Path $native "target\$profile\idmxl.exe") -Force
Write-Output "IDMXL build complete ($profile). Use scripts\start.ps1 to open it."
