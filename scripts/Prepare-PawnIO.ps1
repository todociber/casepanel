[CmdletBinding()]
param([string]$OutFile=(Join-Path (Split-Path -Parent $PSScriptRoot) 'PawnIO_setup.exe'))
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$target=[IO.Path]::GetFullPath($OutFile)
New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
if(-not(Test-Path -LiteralPath $target)){Invoke-WebRequest -Uri 'https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe' -OutFile $target}
if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne '1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032') {throw 'PawnIO installer hash mismatch.'}
$sig=Get-AuthenticodeSignature -LiteralPath $target
if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'CN=namazso.eu'){throw 'Invalid official PawnIO signature.'}
Write-Output "Verified: $target"
