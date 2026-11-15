$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$target=Join-Path $root 'PawnIO_setup.exe'
Invoke-WebRequest -Uri 'https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe' -OutFile $target
if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne '1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032') {throw 'Hash PawnIO inválido.'}
$sig=Get-AuthenticodeSignature -LiteralPath $target
if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'CN=namazso.eu'){throw 'Firma PawnIO inválida.'}
Write-Output "Verificado: $target"
