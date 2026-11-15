#Requires -RunAsAdministrator
param([string]$ProjectRoot=(Split-Path -Parent $PSScriptRoot))
$ErrorActionPreference='Stop'
$out=Join-Path $ProjectRoot 'artifacts\diagnostic-hardware.json'
$exe=Join-Path $ProjectRoot 'target\release\sensorpanel.exe'
if (-not(Test-Path -LiteralPath $exe)) {$exe=Join-Path $ProjectRoot 'sensorpanel.exe'}
if (-not(Test-Path -LiteralPath $exe)) {throw 'No se encontró el ejecutable del diagnóstico.'}
New-Item -ItemType Directory -Path (Split-Path -Parent $out) -Force | Out-Null
$sensorTasks=@(Get-ScheduledTask -TaskPath '\SensorPanel\' -ErrorAction SilentlyContinue | Where-Object {$_.State -eq 'Running'})
$aida=Get-ScheduledTask -TaskName 'AIDA64 AutoStart' -ErrorAction SilentlyContinue
$wasRunning=@(Get-Process -Name aida64 -ErrorAction SilentlyContinue).Count -gt 0
try {
    # Pause the installed collector as well: there must be a single low-level reader.
    foreach ($task in $sensorTasks) {Stop-ScheduledTask -InputObject $task}
    if ($sensorTasks.Count) {Start-Sleep -Seconds 2}
    if ($wasRunning -and $aida) { Stop-ScheduledTask -InputObject $aida; Start-Sleep -Seconds 2 }
    if (@(Get-Process -Name aida64 -ErrorAction SilentlyContinue).Count -gt 0) {throw 'AIDA64 sigue activo. Cierre la aplicación antes de probar el acceso de bajo nivel.'}
    $probe=Start-Process -FilePath $exe -ArgumentList '--diagnose','--samples','5','--out',('"'+$out+'"') -WindowStyle Hidden -PassThru
    if (-not $probe.WaitForExit(20000)) {$probe.Kill();throw 'Diagnóstico excedió 20 segundos; solo se detuvo el proceso de prueba.'}
    if ($probe.ExitCode -ne 0) {throw "Diagnóstico terminó con código $($probe.ExitCode)"}
} catch {
    [pscustomobject]@{error=$_.Exception.Message} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $ProjectRoot 'artifacts\hardware-test-error.json') -Encoding utf8
    throw
} finally {
    if ($wasRunning -and $aida) { Start-ScheduledTask -InputObject $aida }
    foreach ($task in $sensorTasks) {Start-ScheduledTask -InputObject $task}
}
