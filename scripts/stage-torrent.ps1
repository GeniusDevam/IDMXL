param([string]$PythonExe='python', [switch]$Rebuild)
$ErrorActionPreference='Stop'
$torrentRepo=Split-Path -Parent $PSScriptRoot
$torrentOutput=Join-Path $torrentRepo 'apps\desktop\src-tauri\torrent'
$torrentPython=Join-Path $torrentRepo '.local\torrent-build\Scripts\python.exe'
$torrentRecipe=((Get-FileHash -LiteralPath (Join-Path $torrentRepo 'torrent\service.py')).Hash + (Get-FileHash -LiteralPath (Join-Path $torrentRepo 'torrent\requirements.txt')).Hash + (Get-FileHash -LiteralPath $PSCommandPath).Hash)
$torrentMarker=Join-Path $torrentOutput '.idmxl-torrent-recipe'
$torrentMatches=(Test-Path -LiteralPath $torrentMarker) -and (Get-Content -LiteralPath $torrentMarker -Raw).Trim() -eq $torrentRecipe
if ($Rebuild -or -not $torrentMatches -or -not (Test-Path -LiteralPath (Join-Path $torrentOutput 'idmxl-torrent.exe'))) {
    if (-not (Test-Path -LiteralPath $torrentPython)) {
        & $PythonExe -m venv (Join-Path $torrentRepo '.local\torrent-build')
        if ($LASTEXITCODE -ne 0) { throw 'Python 3.12 or newer is required for the torrent build.' }
    }
    & $torrentPython -m pip install -r (Join-Path $torrentRepo 'torrent\requirements.txt')
    if ($LASTEXITCODE -ne 0) { throw 'Torrent build dependencies could not be installed.' }
    & $torrentPython -m PyInstaller --noconfirm --clean --onedir --noconsole --name idmxl-torrent --distpath (Join-Path $torrentRepo '.local\torrent-dist') --workpath (Join-Path $torrentRepo '.local\torrent-pyinstaller') --specpath (Join-Path $torrentRepo '.local') (Join-Path $torrentRepo 'torrent\service.py')
    if ($LASTEXITCODE -ne 0) { throw 'Torrent worker packaging failed.' }
    New-Item -ItemType Directory -Path $torrentOutput -Force | Out-Null
    Copy-Item -Path (Join-Path $torrentRepo '.local\torrent-dist\idmxl-torrent\*') -Destination $torrentOutput -Recurse -Force
    & $torrentPython -m pip freeze | Set-Content -LiteralPath (Join-Path $torrentOutput 'build-dependencies.txt') -Encoding UTF8
    Copy-Item -LiteralPath (Join-Path $torrentRepo 'torrent\notices') -Destination $torrentOutput -Recurse -Force
    Set-Content -LiteralPath $torrentMarker -Value $torrentRecipe -Encoding ASCII
}
Write-Output 'Bundled BitTorrent worker is ready.'
