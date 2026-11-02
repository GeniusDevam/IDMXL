param([Parameter(Mandatory)][string]$Executable)
$ErrorActionPreference='Stop'
$torrentApp=(Resolve-Path -LiteralPath $Executable).Path
if ([IO.Path]::GetFileName($torrentApp) -ne 'idmxl-desktop.exe') {throw 'Choose the installed IDMXL desktop application.'}
$torrentCommand='"'+$torrentApp+'" "%1"'
foreach($torrentKind in @('Torrent','Magnet')) {
    $torrentClass='HKCU:\Software\Classes\IDMXL.'+$torrentKind
    New-Item -Path ($torrentClass+'\shell\open\command') -Force | Out-Null
    Set-Item -LiteralPath $torrentClass -Value ('IDMXL '+$torrentKind)
    Set-Item -LiteralPath ($torrentClass+'\shell\open\command') -Value $torrentCommand
}
New-Item -Path 'HKCU:\Software\Classes\IDMXL.Magnet' -Force | Out-Null
New-ItemProperty -LiteralPath 'HKCU:\Software\Classes\IDMXL.Magnet' -Name 'URL Protocol' -Value '' -PropertyType String -Force | Out-Null
New-Item -Path 'HKCU:\Software\Classes\.torrent\OpenWithProgids' -Force | Out-Null
New-ItemProperty -LiteralPath 'HKCU:\Software\Classes\.torrent\OpenWithProgids' -Name 'IDMXL.Torrent' -Value '' -PropertyType String -Force | Out-Null
New-Item -Path 'HKCU:\Software\IDMXL\Capabilities\FileAssociations' -Force | Out-Null
New-Item -Path 'HKCU:\Software\IDMXL\Capabilities\URLAssociations' -Force | Out-Null
New-ItemProperty -LiteralPath 'HKCU:\Software\IDMXL\Capabilities' -Name ApplicationName -Value IDMXL -PropertyType String -Force | Out-Null
New-ItemProperty -LiteralPath 'HKCU:\Software\IDMXL\Capabilities' -Name ApplicationDescription -Value 'File and BitTorrent downloads' -PropertyType String -Force | Out-Null
New-ItemProperty -LiteralPath 'HKCU:\Software\IDMXL\Capabilities\FileAssociations' -Name '.torrent' -Value 'IDMXL.Torrent' -PropertyType String -Force | Out-Null
New-ItemProperty -LiteralPath 'HKCU:\Software\IDMXL\Capabilities\URLAssociations' -Name magnet -Value 'IDMXL.Magnet' -PropertyType String -Force | Out-Null
New-Item -Path 'HKCU:\Software\RegisteredApplications' -Force | Out-Null
New-ItemProperty -LiteralPath 'HKCU:\Software\RegisteredApplications' -Name IDMXL -Value 'Software\IDMXL\Capabilities' -PropertyType String -Force | Out-Null
Write-Output 'IDMXL is available in Windows Open with and Default apps for torrent files and magnet links. Your existing defaults were preserved.'
