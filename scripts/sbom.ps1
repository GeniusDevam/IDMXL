param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repo '.local\release-inventory' }
. (Join-Path $PSScriptRoot 'tools.ps1')
Initialize-IDMXLTools
python (Join-Path $PSScriptRoot 'sbom.py') --cargo (Get-Command cargo.exe).Source --engine (Join-Path $repo 'target\debug\idmxl.exe') --output $OutputDirectory
if ($LASTEXITCODE -ne 0) { throw 'Inventory generation failed' }
