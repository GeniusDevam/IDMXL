param([Parameter(Mandatory)][string]$BuildDirectory, [int]$Jobs = 2)
$ErrorActionPreference = 'Stop'
if ($Jobs -lt 1 -or $Jobs -gt 8) { throw 'Jobs must be 1 through 8' }
New-Item -ItemType Directory -Path $BuildDirectory -Force | Out-Null
$BuildDirectory = (Resolve-Path -LiteralPath $BuildDirectory).Path
function Fetch-Verified([string]$Url,[string]$Name,[string]$Hash) {
    $path = Join-Path $BuildDirectory $Name
    if (-not (Test-Path -LiteralPath $path)) { Invoke-WebRequest -Uri $Url -OutFile $path }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $Hash) { throw "Hash mismatch: $Name" }
    return $path
}
$compiler = Fetch-Verified 'https://github.com/skeeto/w64devkit/releases/download/v2.10.1/w64devkit-x64-2.10.1.7z.exe' 'w64devkit.7z.exe' 'ADD1EDF984CE4D473CFEB7C52E0AB38DB73A855BBA6C7F564D5E16648EB2896A'
$archive = Fetch-Verified 'https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz' 'ffmpeg-9.0.2.tar.xz' '8C3850283EB25FA026482078A04051E0BE17347B09EF81A0849BEC15A96E002E'
if (-not (Test-Path -LiteralPath (Join-Path $BuildDirectory 'w64devkit\bin\sh.exe'))) {
    # The verified archive is a GUI-subsystem self-extractor; PowerShell must wait explicitly.
    $extractor=Start-Process -FilePath $compiler -ArgumentList @('-y',('-o"{0}"' -f $BuildDirectory)) -Wait -PassThru -WindowStyle Hidden
    if ($extractor.ExitCode -ne 0) { throw 'Compiler extraction failed' }
}
if (-not (Test-Path -LiteralPath (Join-Path $BuildDirectory 'ffmpeg-9.0.2\configure'))) {
    & tar.exe -xf $archive -C $BuildDirectory
    if ($LASTEXITCODE -ne 0) { throw 'Source extraction failed' }
}
$bin = Join-Path $BuildDirectory 'w64devkit\bin'
$priorPath = $env:PATH
$env:PATH = $bin + ';' + $priorPath
$output = Join-Path $BuildDirectory 'remux'
New-Item -ItemType Directory -Path $output -Force | Out-Null
Push-Location $output
try {
    $recipeHash=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'media-build.ps1') -Algorithm SHA256).Hash
    if (-not (Test-Path -LiteralPath '.idmxl-recipe') -or (Get-Content -LiteralPath '.idmxl-recipe' -Raw).Trim() -ne $recipeHash) {
        & (Join-Path $bin 'sh.exe') ../ffmpeg-9.0.2/configure --target-os=mingw32 --arch=x86_64 --cc=gcc --cxx=g++ --disable-everything --disable-autodetect --disable-network --disable-gpl --disable-nonfree --disable-version3 --disable-doc --disable-debug --disable-x86asm --disable-avdevice --enable-protocol=file --enable-demuxer=mov,matroska,mpegts,aac,mp3,ogg,wav --enable-muxer=mp4,matroska,webm,mpegts,adts,mp3,ogg,wav --enable-parser=h264,hevc,av1,vp9,aac,mpegaudio --enable-decoder=aac --enable-bsf=aac_adtstoasc,extract_extradata,h264_mp4toannexb,hevc_mp4toannexb --enable-ffmpeg --enable-ffprobe --enable-static --disable-shared
        if ($LASTEXITCODE -ne 0) { throw 'FFmpeg configuration failed' }
    }
    # Passing V=1 avoids this GNU Make/Windows combination forwarding @printf to sh.
    & (Join-Path $bin 'make.exe') -j $Jobs V=1
    if ($LASTEXITCODE -ne 0) { throw 'FFmpeg build failed' }
    [IO.File]::WriteAllText((Join-Path $output '.idmxl-recipe'),$recipeHash)
    & '.\ffmpeg.exe' -version
    if ($LASTEXITCODE -ne 0) { throw 'Built helper did not run' }
    Get-FileHash -LiteralPath 'ffmpeg.exe','ffprobe.exe' -Algorithm SHA256 | ConvertTo-Json | Set-Content -LiteralPath 'binary-hashes.json' -Encoding UTF8
} finally { Pop-Location; $env:PATH = $priorPath }
