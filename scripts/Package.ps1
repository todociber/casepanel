$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
& (Join-Path $PSScriptRoot 'dev.ps1') build --release --offline
if($LASTEXITCODE -ne 0){throw 'Compilación falló.'}
$package=Join-Path $root 'dist\SensorPanel-0.1.0-win-x64'
New-Item -ItemType Directory -Path $package,(Join-Path $package 'scripts'),(Join-Path $package 'licenses') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'target\release\sensorpanel.exe') -Destination $package -Force
foreach($name in 'README.md','THIRD-PARTY-NOTICES.md','LICENSE'){Copy-Item -LiteralPath (Join-Path $root $name) -Destination $package -Force}
Copy-Item -LiteralPath (Join-Path $root 'docs') -Destination $package -Recurse -Force
foreach($name in 'Install.ps1','Uninstall.ps1','Prepare-PawnIO.ps1','Prepare-Modules.ps1','Measure.ps1','Verify-Startup.ps1','Test-Hardware.ps1'){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $package 'scripts') -Force}
if(Test-Path -LiteralPath (Join-Path $root '.tools\PawnIO_setup.exe')){Copy-Item -LiteralPath (Join-Path $root '.tools\PawnIO_setup.exe') -Destination $package -Force}
Copy-Item -LiteralPath (Join-Path $root 'vendor\pawnio\modules\COPYING') -Destination (Join-Path $package 'licenses\PawnIO-Modules-LGPL-2.1.txt') -Force
$profileProcess=Start-Process -FilePath (Join-Path $package 'sensorpanel.exe') -ArgumentList '--default-profile','--out',('"'+(Join-Path $package 'default-profile.json')+'"') -WindowStyle Hidden -Wait -PassThru
if($profileProcess.ExitCode -ne 0){throw 'No se pudo generar el perfil de muestra.'}
$sourceRoot=Join-Path $package 'source'
New-Item -ItemType Directory -Path $sourceRoot,(Join-Path $sourceRoot '.cargo'),(Join-Path $sourceRoot 'vendor\pawnio\modules') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'src') -Destination $sourceRoot -Recurse -Force
foreach($name in 'Cargo.toml','Cargo.lock','rust-toolchain.toml','build.rs','app.manifest','LICENSE','THIRD-PARTY-NOTICES.md'){Copy-Item -LiteralPath (Join-Path $root $name) -Destination $sourceRoot -Force}
Copy-Item -LiteralPath (Join-Path $root 'vendor\reference') -Destination (Join-Path $sourceRoot 'vendor') -Recurse -Force
Copy-Item -LiteralPath (Join-Path $root '.cargo\config.toml') -Destination (Join-Path $sourceRoot '.cargo') -Force
foreach($name in 'AMDFamily17.bin','LpcIO.bin','Nvidia.bin','COPYING'){Copy-Item -LiteralPath (Join-Path $root "vendor\pawnio\modules\$name") -Destination (Join-Path $sourceRoot 'vendor\pawnio\modules') -Force}
$crateRoot=Join-Path $root '.tools\cargo\registry\src'
if(Test-Path -LiteralPath $crateRoot){Get-ChildItem -LiteralPath $crateRoot -Directory | Get-ChildItem -Directory | ForEach-Object {$crate=$_; $licenseFiles=@(Get-ChildItem -LiteralPath $crate.FullName -File | Where-Object {$_.Name -match '^(LICENSE|COPYING|NOTICE|COPYRIGHT)'});if($licenseFiles.Count){$licenseTarget=Join-Path $package ('licenses\crates\'+$crate.Name);New-Item -ItemType Directory -Path $licenseTarget -Force | Out-Null;foreach($licenseFile in $licenseFiles){Copy-Item -LiteralPath $licenseFile.FullName -Destination $licenseTarget -Force}}}}
Get-ChildItem -LiteralPath $package -File -Recurse | Where-Object {$_.FullName -ne (Join-Path $package 'SHA256.json')} | ForEach-Object {[pscustomobject]@{path=$_.FullName.Substring($package.Length+1);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $package 'SHA256.json') -Encoding utf8
Compress-Archive -Path (Join-Path $package '*') -DestinationPath (Join-Path $root 'dist\SensorPanel-0.1.0-win-x64.zip') -Force
Write-Output $package
