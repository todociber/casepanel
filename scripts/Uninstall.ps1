#Requires -RunAsAdministrator
$ErrorActionPreference='Stop'
$programFiles=[Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
$target=[IO.Path]::GetFullPath((Join-Path $programFiles 'SensorPanel'))
if ($target -ne [IO.Path]::GetFullPath((Join-Path $programFiles 'SensorPanel'))) {throw 'Unsupported directory.'}
if (-not(Test-Path -LiteralPath (Join-Path $target 'installation.json'))) {throw 'The installation receipt was not found.'}
foreach($name in 'Panel','Collector') {
    Stop-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -Confirm:$false -ErrorAction SilentlyContinue
}
# Explicit files only; no recursive deletion, no profile or shared PawnIO removal.
foreach($name in 'sensorpanel.exe','README.md','THIRD-PARTY-NOTICES.md','LICENSE','installation.json','Uninstall.ps1') {
    $file=Join-Path $target $name
    if(Test-Path -LiteralPath $file){Remove-Item -LiteralPath $file -Force}
}
foreach($name in 'ARCHITECTURE.md','VALIDATION.md') {
    $file=Join-Path $target "docs\$name"
    if(Test-Path -LiteralPath $file){Remove-Item -LiteralPath $file -Force}
}
Write-Output 'Tasks and application removed. Profiles, PawnIO, and the AIDA64 backup were preserved.'
