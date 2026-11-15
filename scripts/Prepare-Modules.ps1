[CmdletBinding()]
param([string]$ProjectRoot=(Split-Path -Parent $PSScriptRoot),[switch]$IncludeSource)
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
    throw 'Module archive does not match the pinned SHA-256; extraction cancelled.'
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::OpenRead($archive)
try {
    # Explicit names: do not extract arbitrary paths from the archive.
    foreach($name in 'AMDFamily17.bin','LpcIO.bin','Nvidia.bin','COPYING') {
        $entry=$zip.GetEntry($name)
        if(-not $entry){throw "The official distribution is missing $name."}
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,(Join-Path $moduleRoot $name),$true)
    }
} finally {$zip.Dispose()}
if($IncludeSource){
    $source=Join-Path $cache 'PawnIO.Modules-source-0.2.11.zip'
    if(-not(Test-Path -LiteralPath $source)){Invoke-WebRequest -Uri 'https://codeload.github.com/namazso/PawnIO.Modules/zip/52a7e536dff3e53c96917a28caac5e0fa6510696' -OutFile $source}
    if((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne '3766588ABEC0A353AA8824A1F138A443A4E54C62A38EEC30B2D44D770C59E62A'){throw 'Module source archive does not match the pinned SHA-256.'}
}
Write-Output "Official modules $version verified and prepared in $moduleRoot"
