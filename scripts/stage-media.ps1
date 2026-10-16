param([string]$BuildDirectory)
$ErrorActionPreference='Stop'
$mediaRepo=Split-Path -Parent $PSScriptRoot
if (-not $BuildDirectory) { $BuildDirectory=Join-Path $mediaRepo '..\..\work\media-build' }
$BuildDirectory=(Resolve-Path -LiteralPath $BuildDirectory).Path
$mediaRecipe=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'media-build.ps1') -Algorithm SHA256).Hash
$mediaMarker=Join-Path $BuildDirectory 'remux\.idmxl-recipe'
if (-not (Test-Path -LiteralPath $mediaMarker) -or (Get-Content -LiteralPath $mediaMarker -Raw).Trim() -ne $mediaRecipe) { throw 'Media helpers do not match the current recipe. Run scripts/media-build.ps1 first.' }
$mediaSource=Join-Path $BuildDirectory 'ffmpeg-9.0.2.tar.xz'
if ((Get-FileHash -LiteralPath $mediaSource -Algorithm SHA256).Hash -ne '8C3850283EB25FA026482078A04051E0BE17347B09EF81A0849BEC15A96E002E') { throw 'Unexpected FFmpeg source archive.' }
$env:IDMXL_MEDIA_FFMPEG_SHA256=(Get-FileHash -LiteralPath (Join-Path $BuildDirectory 'remux\ffmpeg.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
$env:IDMXL_MEDIA_FFPROBE_SHA256=(Get-FileHash -LiteralPath (Join-Path $BuildDirectory 'remux\ffprobe.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
$mediaResource=Join-Path $mediaRepo 'apps\desktop\src-tauri\media'
New-Item -ItemType Directory -Path $mediaResource -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $BuildDirectory 'remux\ffmpeg.exe'),(Join-Path $BuildDirectory 'remux\ffprobe.exe'),$mediaSource -Destination $mediaResource -Force
Copy-Item -LiteralPath (Join-Path $BuildDirectory 'ffmpeg-9.0.2\COPYING.LGPLv2.1'),(Join-Path $BuildDirectory 'w64devkit\COPYING.MinGW-w64-runtime.txt'),(Join-Path $PSScriptRoot 'media-build.ps1') -Destination $mediaResource -Force
[IO.File]::WriteAllText((Join-Path $mediaResource 'BUILD-AND-SOURCE.txt'),"IDMXL standalone media helpers: FFmpeg 9.0.2, LGPL-2.1-or-later. Source archive and complete local build recipe are included. Network, encoders, GPL, nonfree and version-3 components are disabled. AAC decoding is enabled for stream information; all assembly uses stream copy. w64devkit 2.10.1/GCC 16.2.0 build tools are pinned in the recipe. See https://ffmpeg.org/legal.html and https://github.com/skeeto/w64devkit. GCC Runtime Library Exception: https://www.gnu.org/licenses/gcc-exception-3.1.html. Exact shipped helper hashes are embedded in IDMXL by the build step.")
foreach ($mediaProfile in @('debug','release')) {
    foreach ($mediaTarget in @((Join-Path $mediaRepo "target\$mediaProfile\media"),(Join-Path $mediaRepo "apps\desktop\src-tauri\target\$mediaProfile\media"))) {
        New-Item -ItemType Directory -Path $mediaTarget -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $mediaResource 'ffmpeg.exe'),(Join-Path $mediaResource 'ffprobe.exe') -Destination $mediaTarget -Force
    }
}
Write-Output 'Staged the controlled media helpers, matching source and notices.'
