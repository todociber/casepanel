[CmdletBinding()]
param([string]$ProjectRoot=(Split-Path -Parent $PSScriptRoot))
$ErrorActionPreference='Stop'
$version='0.2.11'
$expectedHash='43608CB89BC84247FEF1368A139013F7D043E17DB6D6C8DFC9B46BF0905A81F4'
$cache=Join-Path $ProjectRoot '.tools\downloads'
$archive=Join-Path $cache 'PawnIO.Modules-0.2.11.zip'
$moduleRoot=Join-Path $ProjectRoot 'vendor\pawnio\modules'
New-Item -ItemType Directory -Path $cache,$moduleRoot -Force | Out-Null
if(-not(Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri 'https://github.com/namazso/PawnIO.Modules/releases/download/0.2.11/release_0_2_11.zip' -OutFile $archive
}
if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'El archivo de módulos no coincide con el SHA-256 fijado. Se cancela la extracción.'
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::OpenRead($archive)
try {
    # Explicit names: do not extract arbitrary paths from the archive.
    foreach($name in 'AMDFamily17.bin','LpcIO.bin','Nvidia.bin','COPYING') {
        $entry=$zip.GetEntry($name)
        if(-not $entry){throw "Falta $name en la distribución oficial."}
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,(Join-Path $moduleRoot $name),$true)
    }
} finally {$zip.Dispose()}
Write-Output "Módulos oficiales $version verificados y preparados en $moduleRoot"
