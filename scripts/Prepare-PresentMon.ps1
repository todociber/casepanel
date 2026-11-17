[CmdletBinding()]
param([string]$OutFile=(Join-Path (Split-Path -Parent $PSScriptRoot) 'vendor\presentmon\PresentMon.exe'))
$ErrorActionPreference='Stop'
$target=[IO.Path]::GetFullPath($OutFile)
New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
if(-not(Test-Path -LiteralPath $target)) {
    Invoke-WebRequest -Uri 'https://github.com/GameTechDev/PresentMon/releases/download/v2.3.1/PresentMon-2.3.1-x64.exe' -OutFile $target
}
if((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne '364E5D98D4D134BD54DD25C22ED2CA2F4883F8BC3ED6502BEE0C151E3436D30C') {
    throw 'PresentMon checksum mismatch.'
}
Write-Output "Verified PresentMon 2.3.1: $target"
