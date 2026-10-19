param([switch]$Faults, [switch]$Protocols,[switch]$Media,[string]$MediaBuildDirectory)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'tools.ps1')
Initialize-IDMXLTools
if ($Media) {
    if (-not $MediaBuildDirectory) { $MediaBuildDirectory=Join-Path $repo '..\..\work\media-build' }
    & (Join-Path $PSScriptRoot 'media-build.ps1') -BuildDirectory $MediaBuildDirectory
    if (-not (Test-Path -LiteralPath (Join-Path $MediaBuildDirectory 'fixture-encoder\ffmpeg.exe'))) { & (Join-Path $PSScriptRoot 'media-fixture-build.ps1') -BuildDirectory $MediaBuildDirectory }
    $env:IDMXL_FIXTURE_ENCODER=Join-Path $MediaBuildDirectory 'fixture-encoder\ffmpeg.exe'
    . (Join-Path $PSScriptRoot 'stage-media.ps1') -BuildDirectory $MediaBuildDirectory
    $env:IDMXL_REQUIRE_MEDIA_TESTS='1'
}
Push-Location $repo
try {
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed.' }
    cargo clippy --workspace --all-targets --locked -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust lint check failed.' }
    cargo test --workspace --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    if ($Faults -or $Protocols) {
        cargo build --workspace --locked --features fault-injection
        if ($LASTEXITCODE -ne 0) { throw 'Fault-test build failed.' }
        New-Item -ItemType Directory -Path '.test-output' -Force | Out-Null
        Copy-Item -LiteralPath 'target\debug\idmxl.exe' -Destination '.test-output\idmxl-fault.exe' -Force
        $env:IDMXL_FAULT_EXE = Join-Path $repo '.test-output\idmxl-fault.exe'
    }
    if ($Protocols) {
        python -c 'import h2, cryptography'
        if ($LASTEXITCODE -ne 0) { throw 'Install tests/requirements.txt to run protocol fixtures.' }
        $env:IDMXL_PROTOCOL_TESTS = '1'
    }
    cargo build --workspace --locked
    if ($LASTEXITCODE -ne 0) { throw 'CLI build failed.' }
    python -m unittest discover -s tests -v
    if ($LASTEXITCODE -ne 0) { throw 'HTTP fixture tests failed.' }
    node --test extensions/edge/background.test.cjs extensions/edge/capture.test.cjs extensions/edge/capture-settings.test.cjs extensions/edge/capture-manager.test.cjs extensions/edge/popup.test.cjs extensions/edge/capture-status.test.cjs extensions/edge/browser-bridge.test.cjs
    if ($LASTEXITCODE -ne 0) { throw 'Extension behavior tests failed.' }
    if (-not (Test-Path -LiteralPath 'apps\desktop\node_modules')) {
        npm.cmd --prefix apps/desktop ci --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed.' }
    }
    npm.cmd --prefix apps/desktop run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
    node --test tests/links.test.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Link import tests failed.' }
} finally {
    Remove-Item Env:IDMXL_FAULT_EXE -ErrorAction SilentlyContinue
    Remove-Item Env:IDMXL_PROTOCOL_TESTS -ErrorAction SilentlyContinue
    Remove-Item Env:IDMXL_REQUIRE_MEDIA_TESTS -ErrorAction SilentlyContinue
    Pop-Location
}
