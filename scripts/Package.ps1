[CmdletBinding()]
param([switch]$SkipBuild,[switch]$Offline,[string]$ExpectedVersion='')
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
$versionMatch=[regex]::Match([IO.File]::ReadAllText((Join-Path $root 'Cargo.toml')),'(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?)"')
if(-not $versionMatch.Success){throw 'Cannot read the package version.'}
$version=$versionMatch.Groups[1].Value
if($ExpectedVersion -and $ExpectedVersion -ne $version){throw 'Tag version does not match Cargo.toml.'}
& (Join-Path $PSScriptRoot 'Prepare-Modules.ps1') -IncludeSource
$cache=Join-Path $root '.tools\downloads'
$installer=Join-Path $cache 'PawnIO_setup.exe'
& (Join-Path $PSScriptRoot 'Prepare-PawnIO.ps1') -OutFile $installer
if(-not $SkipBuild){
    $buildArgs=@('build','--release','--locked')
    if($Offline){$buildArgs+='--offline'}
    & (Join-Path $PSScriptRoot 'dev.ps1') @buildArgs
    if($LASTEXITCODE -ne 0){throw 'Release build failed.'}
} else {
    & (Join-Path $PSScriptRoot 'dev.ps1') --version | Out-Null
    if($LASTEXITCODE -ne 0){throw 'Cargo is unavailable.'}
}
$metadataArgs=@('metadata','--format-version','1','--locked')
if($Offline){$metadataArgs+='--offline'}
$metadataText=& cargo @metadataArgs
if($LASTEXITCODE -ne 0){throw 'Cannot resolve dependency license metadata.'}
$metadata=$metadataText|ConvertFrom-Json
$dist=Join-Path $root 'dist'
$name="CasePanel-$version-windows-x64"
# A fresh staging directory prevents old files from leaking into a new package.
$stage=Join-Path $dist ('staging\'+[guid]::NewGuid().ToString('N'))
$package=Join-Path $stage $name
$sourceRoot=Join-Path $package 'source'
$licenses=Join-Path $package 'licenses'
New-Item -ItemType Directory -Path $package,$sourceRoot,$licenses,(Join-Path $package 'scripts'),(Join-Path $sourceRoot 'vendor\pawnio\modules') -Force|Out-Null
$exe=Join-Path $root 'target\release\sensorpanel.exe'
if(-not(Test-Path -LiteralPath $exe)){throw 'The release executable is missing.'}
Copy-Item -LiteralPath $exe -Destination $package -Force
Copy-Item -LiteralPath $installer -Destination (Join-Path $package 'PawnIO_setup.exe') -Force
foreach($file in 'README.md','THIRD-PARTY-NOTICES.md','LICENSE'){Copy-Item -LiteralPath (Join-Path $root $file) -Destination $package -Force}
Copy-Item -LiteralPath (Join-Path $root 'docs') -Destination $package -Recurse -Force
foreach($file in 'Install.ps1','Uninstall.ps1','Prepare-PawnIO.ps1','Prepare-Modules.ps1','Measure.ps1','Verify-Startup.ps1','Test-Hardware.ps1'){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination (Join-Path $package 'scripts') -Force}
Copy-Item -LiteralPath (Join-Path $root 'vendor\pawnio\modules\COPYING') -Destination (Join-Path $licenses 'PawnIO-Modules-LGPL-2.1.txt') -Force
$moduleSource=Join-Path $cache 'PawnIO.Modules-source-0.2.11.zip'
Copy-Item -LiteralPath $moduleSource -Destination $licenses -Force
foreach($crate in $metadata.packages){
    if($crate.name -eq 'sensorpanel'){continue}
    $crateDir=Split-Path -Parent $crate.manifest_path
    $licenseFiles=@(Get-ChildItem -LiteralPath $crateDir -File|Where-Object {$_.Name -match '^(LICENSE|COPYING|NOTICE|COPYRIGHT)'})
    $target=Join-Path $licenses ('crates\'+$crate.name+'-'+$crate.version)
    New-Item -ItemType Directory -Path $target -Force|Out-Null
    foreach($file in $licenseFiles){Copy-Item -LiteralPath $file.FullName -Destination $target -Force}
    Copy-Item -LiteralPath $crate.manifest_path -Destination $target -Force
}
foreach($directory in 'src','.cargo','docs','scripts','.github'){
    $path=Join-Path $root $directory
    if(Test-Path -LiteralPath $path){Copy-Item -LiteralPath $path -Destination $sourceRoot -Recurse -Force}
}
foreach($file in 'Cargo.toml','Cargo.lock','rust-toolchain.toml','build.rs','app.manifest','README.md','LICENSE','THIRD-PARTY-NOTICES.md','.gitignore','.gitattributes'){Copy-Item -LiteralPath (Join-Path $root $file) -Destination $sourceRoot -Force}
$referenceTarget=Join-Path $sourceRoot 'vendor\reference'
New-Item -ItemType Directory -Path $referenceTarget -Force|Out-Null
Get-ChildItem -LiteralPath (Join-Path $root 'vendor\reference') -File|Where-Object {$_.Name -ne 'nvml.h'}|ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination $referenceTarget -Force}
foreach($file in 'AMDFamily17.bin','LpcIO.bin','Nvidia.bin','COPYING'){Copy-Item -LiteralPath (Join-Path $root "vendor\pawnio\modules\$file") -Destination (Join-Path $sourceRoot 'vendor\pawnio\modules') -Force}
Copy-Item -LiteralPath $moduleSource -Destination (Join-Path $sourceRoot 'vendor\pawnio') -Force
Copy-Item -LiteralPath $licenses -Destination $sourceRoot -Recurse -Force
$commit=& git rev-parse HEAD
if($LASTEXITCODE -ne 0){throw 'Cannot identify the build commit.'}
$dirty=@(& git status --porcelain --untracked-files=no).Count -gt 0
[pscustomobject]@{version=$version;target='x86_64-pc-windows-msvc';commit=$commit;uncommitted_changes=$dirty;module_source_commit='52a7e536dff3e53c96917a28caac5e0fa6510696';executable_signed=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $package 'build-info.json') -Encoding utf8
$profile=Start-Process -FilePath (Join-Path $package 'sensorpanel.exe') -ArgumentList '--default-profile','--out',('"'+(Join-Path $package 'default-profile.json')+'"') -WindowStyle Hidden -Wait -PassThru
if($profile.ExitCode -ne 0){throw 'Cannot generate the default profile.'}
Get-ChildItem -LiteralPath $package -File -Recurse|ForEach-Object {if($_.LastWriteTime.Year -lt 1980){$_.LastWriteTime=[datetime]::new(1980,1,2)}}
Get-ChildItem -LiteralPath $package -File -Recurse|ForEach-Object {[pscustomobject]@{path=$_.FullName.Substring($package.Length+1).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $package 'SHA256.json') -Encoding utf8
$zipPath=Join-Path $dist "$name.zip"
$sourceZip=Join-Path $dist "CasePanel-$version-source.zip"
Compress-Archive -Path (Join-Path $package '*') -DestinationPath $zipPath -Force
Compress-Archive -Path (Join-Path $sourceRoot '*') -DestinationPath $sourceZip -Force
$standalone=Join-Path $dist 'sensorpanel.exe'
Copy-Item -LiteralPath $exe -Destination $standalone -Force
$checksums=@(foreach($path in $zipPath,$sourceZip,$standalone){$hash=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant();$hash+'  '+[IO.Path]::GetFileName($path)})
[IO.File]::WriteAllLines((Join-Path $dist 'SHA256SUMS.txt'),$checksums,[Text.UTF8Encoding]::new($false))
[pscustomobject]@{version=$version;package_directory=$package;windows_zip=$zipPath;source_zip=$sourceZip;executable=$standalone;checksums=(Join-Path $dist 'SHA256SUMS.txt')}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $dist 'release-assets.json') -Encoding utf8
Write-Output "Release assets prepared in $dist"
