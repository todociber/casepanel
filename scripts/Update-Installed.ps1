#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [string]$Source=(Split-Path -Parent $PSScriptRoot),
    [string]$ProfilePath=(Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'SensorPanel\profile.json'),
    [string]$ReportPath=''
)
$ErrorActionPreference='Stop'
$installRoot=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'SensorPanel'
$installedExe=Join-Path $installRoot 'sensorpanel.exe'
$sourceExe=Join-Path $Source 'target\release\sensorpanel.exe'
if(-not(Test-Path -LiteralPath $sourceExe)) {$sourceExe=Join-Path $Source 'sensorpanel.exe'}
$presentMon=Join-Path $Source 'vendor\presentmon\PresentMon.exe'
if(-not(Test-Path -LiteralPath $presentMon)) {$presentMon=Join-Path $Source 'PresentMon.exe'}
if(-not(Test-Path -LiteralPath $installedExe)) {throw 'No existing CasePanel installation. Run Install.ps1 instead.'}
if(-not(Test-Path -LiteralPath $sourceExe)) {throw 'Build the release executable first.'}
$sourceVersion=''
$buildInfo=Join-Path $Source 'build-info.json'
if(Test-Path -LiteralPath $buildInfo){$sourceVersion=(Get-Content -LiteralPath $buildInfo -Raw|ConvertFrom-Json).version}
elseif(Test-Path -LiteralPath (Join-Path $Source 'Cargo.toml')){$sourceVersion=[regex]::Match([IO.File]::ReadAllText((Join-Path $Source 'Cargo.toml')),'(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value}
if(-not $sourceVersion){throw 'Cannot identify the update version.'}
if((Get-FileHash -LiteralPath $presentMon -Algorithm SHA256).Hash -ne '364E5D98D4D134BD54DD25C22ED2CA2F4883F8BC3ED6502BEE0C151E3436D30C') {throw 'PresentMon checksum mismatch.'}
$backup=Join-Path $installRoot ('backups\'+(Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $backup -Force | Out-Null
$replaced=@('sensorpanel.exe','PresentMon.exe','PresentMon-LICENSE.txt','README.md','THIRD-PARTY-NOTICES.md','installation.json')
foreach($file in $replaced) {
    $old=Join-Path $installRoot $file
    if(Test-Path -LiteralPath $old) {Copy-Item -LiteralPath $old -Destination $backup -Force}
}
$hasProfile=Test-Path -LiteralPath $ProfilePath
if($hasProfile) {Copy-Item -LiteralPath $ProfilePath -Destination (Join-Path $backup 'profile.before-footer.json') -Force}
$tasks=@(Get-ScheduledTask -TaskPath '\SensorPanel\' | Where-Object {$_.TaskName -in @('Panel','Collector')})
$running=@($tasks | Where-Object {$_.State -eq 'Running'} | ForEach-Object TaskName)
$processes=@(Get-CimInstance Win32_Process -Filter "Name='sensorpanel.exe'" | Where-Object {$_.ExecutablePath -eq $installedExe})
try {
    foreach($name in 'Panel','Collector') {Stop-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue}
    foreach($old in $processes) {
        $process=Get-Process -Id $old.ProcessId -ErrorAction SilentlyContinue
        if($process -and -not $process.WaitForExit(15000)) {throw 'An installed process did not stop; update cancelled.'}
    }
    Copy-Item -LiteralPath $sourceExe -Destination $installedExe -Force
    Copy-Item -LiteralPath $presentMon -Destination (Join-Path $installRoot 'PresentMon.exe') -Force
    foreach($file in 'README.md','THIRD-PARTY-NOTICES.md') {
        Copy-Item -LiteralPath (Join-Path $Source $file) -Destination $installRoot -Force
    }
    $license=Join-Path $Source 'vendor\presentmon\LICENSE.txt'
    if(-not(Test-Path -LiteralPath $license)) {$license=Join-Path $Source 'licenses\PresentMon-MIT.txt'}
    Copy-Item -LiteralPath $license -Destination (Join-Path $installRoot 'PresentMon-LICENSE.txt') -Force
    $hash=(Get-FileHash -LiteralPath $sourceExe -Algorithm SHA256).Hash
    if((Get-FileHash -LiteralPath $installedExe -Algorithm SHA256).Hash -ne $hash) {throw 'Installed executable verification failed.'}
    if($hasProfile) {
        $migration=Start-Process -FilePath $installedExe -ArgumentList '--upgrade-profile','--profile',('"'+$ProfilePath+'"') -WindowStyle Hidden -PassThru -Wait
        if($migration.ExitCode -ne 0) {throw 'Profile migration failed.'}
    }
    $metadataPath=Join-Path $installRoot 'installation.json'
    if(Test-Path -LiteralPath $metadataPath) {
        $metadata=Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
        $metadata.exe_sha256=$hash
        $metadata | Add-Member -NotePropertyName version -NotePropertyValue $sourceVersion -Force
        $metadata | Add-Member -NotePropertyName updated_at -NotePropertyValue ((Get-Date).ToUniversalTime().ToString('o')) -Force
        $metadata | Add-Member -NotePropertyName presentmon_version -NotePropertyValue '2.3.1' -Force
        $metadata | ConvertTo-Json | Set-Content -LiteralPath $metadataPath -Encoding utf8
    }
    foreach($name in 'Collector','Panel') {if($name -in $running) {Start-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name}}
    $result=[pscustomobject]@{status='complete';exe_sha256=$hash;backup=$backup;profile=$ProfilePath;restarted=$running}
} catch {
    $failure=$_.Exception.Message
    foreach($name in 'Panel','Collector') {Stop-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue}
    foreach($file in $replaced) {
        $old=Join-Path $backup $file
        if(Test-Path -LiteralPath $old) {Copy-Item -LiteralPath $old -Destination (Join-Path $installRoot $file) -Force}
    }
    if($hasProfile) {Copy-Item -LiteralPath (Join-Path $backup 'profile.before-footer.json') -Destination $ProfilePath -Force}
    foreach($name in 'Collector','Panel') {if($name -in $running) {Start-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue}}
    $result=[pscustomobject]@{status='failed';error=$failure;backup=$backup}
}
if($ReportPath) {$result | ConvertTo-Json | Set-Content -LiteralPath $ReportPath -Encoding utf8}
$result | ConvertTo-Json
if($result.status -ne 'complete') {throw $result.error}
