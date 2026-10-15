# Prefer an installed Rust toolchain; allow a portable toolchain as a fallback.
function Initialize-IDMXLTools {
    $repo = Split-Path -Parent $PSScriptRoot
    if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
        $portableRoot = [System.IO.Path]::GetFullPath((Join-Path $repo '..\..\work\toolchains'))
        $portableCargo = Join-Path $portableRoot 'cargo\bin\cargo.exe'
        if (-not (Test-Path -LiteralPath $portableCargo)) {
            throw 'Rust is required. Install rustup and the Microsoft C++ Build Tools, then reopen your shell.'
        }
        $env:CARGO_HOME = Join-Path $portableRoot 'cargo'
        $env:RUSTUP_HOME = Join-Path $portableRoot 'rustup'
        $env:PATH = (Join-Path $portableRoot 'cargo\bin') + [System.IO.Path]::PathSeparator + $env:PATH
    }
}
