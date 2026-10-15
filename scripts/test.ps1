$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'tools.ps1')
Initialize-IDMXLTools
Push-Location $repo
try {
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed.' }
    cargo clippy --workspace --all-targets --locked -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust lint check failed.' }
    cargo test --workspace --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    cargo build --workspace --locked
    if ($LASTEXITCODE -ne 0) { throw 'CLI build failed.' }
    python -m unittest discover -s tests -v
    if ($LASTEXITCODE -ne 0) { throw 'HTTP fixture tests failed.' }
    if (-not (Test-Path -LiteralPath 'apps\desktop\node_modules')) {
        npm.cmd --prefix apps/desktop ci --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed.' }
    }
    npm.cmd --prefix apps/desktop run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
} finally { Pop-Location }
