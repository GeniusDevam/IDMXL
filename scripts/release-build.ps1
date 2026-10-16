param([Parameter(Mandatory)][string]$Endpoint,[Parameter(Mandatory)][string]$ArtifactOrigin,[Parameter(Mandatory)][string]$PublicKeyFile,
  [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{40}$')][string]$CertificateThumbprint,[Parameter(Mandatory)][string]$TimestampUrl,[string]$MediaBuildDirectory)
$ErrorActionPreference='Stop'
$releaseRepo=Split-Path -Parent $PSScriptRoot
foreach ($releaseAddress in @($Endpoint,$ArtifactOrigin,$TimestampUrl)) { $releaseUri=[Uri]$releaseAddress; if ($releaseUri.Scheme -ne 'https' -or $releaseUri.UserInfo -or $releaseUri.Fragment) { throw 'Release addresses must use HTTPS without embedded credentials.' } }
$certificate=Get-Item -LiteralPath "Cert:\CurrentUser\My\$CertificateThumbprint"
if (-not $certificate.HasPrivateKey -or $certificate.NotAfter -le (Get-Date)) { throw 'A current Windows code-signing certificate with a usable private key is required.' }
$publicKey=(Get-Content -LiteralPath $PublicKeyFile -Raw).Trim()
if ([Convert]::FromBase64String($publicKey).Length -ne 32) { throw 'The Ed25519 public key must contain 32 bytes.' }
$releaseFolder=Join-Path $releaseRepo '.local\release';New-Item -ItemType Directory -Path $releaseFolder -Force | Out-Null
$configPath=Join-Path $releaseFolder 'tauri-release.json'
@{bundle=@{windows=@{certificateThumbprint=$CertificateThumbprint;digestAlgorithm='sha256';timestampUrl=$TimestampUrl}}} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $configPath -Encoding UTF8
$oldReleaseConfig=$env:IDMXL_RELEASE_CONFIG
try {
    $env:IDMXL_RELEASE_CONFIG=@{endpoint=$Endpoint;artifact_origin=$ArtifactOrigin.TrimEnd('/');public_key=$publicKey;certificate_thumbprint=$CertificateThumbprint;channel='stable'} | ConvertTo-Json -Compress
    & (Join-Path $PSScriptRoot 'build.ps1') -Bundle -TauriConfig $configPath -MediaBuildDirectory $MediaBuildDirectory
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $releaseInstallers=@(Get-ChildItem -LiteralPath (Join-Path $releaseRepo 'apps\desktop\src-tauri\target\release\bundle\nsis') -Filter '*-setup.exe')
    if ($releaseInstallers.Count -ne 1) { throw 'Expected exactly one installer; clear stale installers before release preparation.' }
    $releaseSignature=Get-AuthenticodeSignature -LiteralPath $releaseInstallers[0].FullName
    if ($releaseSignature.Status -ne 'Valid' -or $releaseSignature.SignerCertificate.Thumbprint -ne $CertificateThumbprint) { throw 'The installer signature did not validate.' }
    # Tauri restores the unpatched checkout executable after signing/bundling the NSIS copy.
    Write-Output 'Signed installer prepared. In the release qualification VM, validate the installed application signature and update/recovery round trip. Then fill release-manifest.example.json with the final installer hash/size/version/sequence and sign it with sign-release.py. Nothing has been published.'
} finally { $env:IDMXL_RELEASE_CONFIG=$oldReleaseConfig }
