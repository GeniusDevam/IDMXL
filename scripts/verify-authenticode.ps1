param([Parameter(Mandatory)][string]$Installer,[Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{40}$')][string]$Thumbprint)
$ErrorActionPreference='Stop'
$signature=Get-AuthenticodeSignature -LiteralPath $Installer
if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Thumbprint -ne $Thumbprint) { exit 1 }
exit 0
