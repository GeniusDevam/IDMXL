param([Parameter(Mandatory)][string]$Installer,[Parameter(Mandatory)][string]$InstalledDirectory,[Parameter(Mandatory)][string]$RecoveryDirectory,
  [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$Sha256,[Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{40}$')][string]$Thumbprint,[int]$ParentProcessId)
$ErrorActionPreference='Stop'
try {
    if ($ParentProcessId -gt 0) { Wait-Process -Id $ParentProcessId -Timeout 120 -ErrorAction SilentlyContinue }
    if ($ParentProcessId -gt 0 -and (Get-Process -Id $ParentProcessId -ErrorAction SilentlyContinue)) { throw 'The app did not exit.' }
    if ((Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash -ne $Sha256) { throw 'Installer changed.' }
    $signature=Get-AuthenticodeSignature -LiteralPath $Installer
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $Thumbprint) { throw 'Installer trust check failed.' }
    # NSIS requires /D last and consumes the remaining unquoted directory text.
    $process=Start-Process -FilePath $Installer -ArgumentList @('/S',"/D=$InstalledDirectory") -PassThru -Wait -WindowStyle Hidden
    if ($process.ExitCode -ne 0) { throw 'Installer returned a failure.' }
    $app=Join-Path $InstalledDirectory 'idmxl-desktop.exe'
    $installedSignature=Get-AuthenticodeSignature -LiteralPath $app
    if ($installedSignature.Status -ne 'Valid' -or $installedSignature.SignerCertificate.Thumbprint -ne $Thumbprint) { throw 'Installed app trust check failed.' }
    @{status='installed'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $RecoveryDirectory 'install-result.json') -Encoding UTF8
    Start-Process -FilePath $app
} catch {
    @{status='failed';recovery='Use the paired backup. The current profile has not been automatically rolled back.'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $RecoveryDirectory 'install-result.json') -Encoding UTF8
    exit 1
}
