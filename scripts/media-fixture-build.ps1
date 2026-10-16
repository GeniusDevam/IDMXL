param([string]$BuildDirectory)
# Development-only AAC fixture encoder. Never included in the installer.
$ErrorActionPreference='Stop'
$BuildDirectory=(Resolve-Path -LiteralPath $BuildDirectory).Path
$fixturePriorPath=$env:PATH
$bin=Join-Path $BuildDirectory 'w64devkit\bin';$env:PATH=$bin+';'+$env:PATH
$output=Join-Path $BuildDirectory 'fixture-encoder';New-Item -ItemType Directory -Path $output -Force | Out-Null
Push-Location $output
try {
 & (Join-Path $bin 'sh.exe') ../ffmpeg-9.0.2/configure --target-os=mingw32 --arch=x86_64 --cc=gcc --disable-everything --disable-autodetect --disable-network --disable-gpl --disable-nonfree --disable-version3 --disable-doc --disable-debug --disable-x86asm --disable-avdevice --enable-protocol=file --enable-demuxer=wav --enable-decoder=pcm_s16le --enable-encoder=aac --enable-muxer=adts,mp4 --enable-filter=aformat,aresample,anull,atrim --enable-ffmpeg --disable-ffprobe --enable-static --disable-shared
 if($LASTEXITCODE -ne 0){throw 'Fixture configuration failed'}
 & (Join-Path $bin 'make.exe') -j 2 V=1
 if($LASTEXITCODE -ne 0){throw 'Fixture build failed'}
} finally {Pop-Location;$env:PATH=$fixturePriorPath}
