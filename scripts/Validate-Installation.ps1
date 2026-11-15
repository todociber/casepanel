#Requires -RunAsAdministrator
param([string]$ProjectRoot=(Split-Path -Parent $PSScriptRoot))
$ErrorActionPreference='Stop'
$artifactRoot=Join-Path $ProjectRoot 'artifacts'
$progress=Join-Path $artifactRoot 'installation-validation-status.json'
$installedExe=Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)) 'SensorPanel\sensorpanel.exe'
function Write-Phase([string]$Phase,[string]$Detail='') {
    [pscustomobject]@{phase=$Phase;detail=$Detail;at=(Get-Date).ToUniversalTime().ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath $progress -Encoding utf8
}
function Find-PanelProcesses {
    @(Get-CimInstance Win32_Process -Filter "Name='sensorpanel.exe'" | Where-Object {$_.ExecutablePath -eq $installedExe})
}
$aida=Get-ScheduledTask -TaskName 'AIDA64 AutoStart' -ErrorAction SilentlyContinue
$aidaWasRunning=@(Get-Process -Name aida64 -ErrorAction SilentlyContinue).Count -gt 0
$cutover=$false
try {
    Write-Phase 'installing'
    & (Join-Path $PSScriptRoot 'Install.ps1') -Source $ProjectRoot
    if($aidaWasRunning -and $aida){Stop-ScheduledTask -InputObject $aida;Start-Sleep -Seconds 2}
    if(@(Get-Process -Name aida64 -ErrorAction SilentlyContinue).Count){throw 'AIDA64 sigue activo; no se iniciará la prueba de placa.'}
    $collectorTask=Get-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Collector'
    $logonTriggers=@($collectorTask.Triggers)
    $scheduledProbe=New-ScheduledTaskTrigger -Once -At (Get-Date).AddSeconds(6)
    Set-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Collector' -Trigger @($logonTriggers+$scheduledProbe) | Out-Null
    Start-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Panel'
    Start-Sleep -Seconds 12
    $processes=Find-PanelProcesses
    $collector=$processes|Where-Object {$_.CommandLine -match '\-\-collector'}|Select-Object -First 1
    $panel=$processes|Where-Object {$_.CommandLine -match '\-\-panel'}|Select-Object -First 1
    if(-not $collector -or -not $panel){throw 'No arrancaron ambos procesos desde sus tareas.'}
    $preflight=(@($collector.ProcessId,$panel.ProcessId)|ForEach-Object {(Get-Process -Id $_).PrivateMemorySize64}|Measure-Object -Sum).Sum/1MB
    if($preflight -gt 80){throw "La reserva inicial de memoria ($([math]::Round($preflight,1)) MiB) requiere ajustar el renderizador antes de medir."}
    Write-Phase 'testing_recovery' "Se interrumpirá el recolector $($collector.ProcessId); el panel $($panel.ProcessId) debe permanecer activo."
    # Match the collector watchdog's exit code rather than PowerShell Kill's -1.
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class SensorPanelFaultTest {
    [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint access, bool inherit, uint id);
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool TerminateProcess(IntPtr process, uint code);
    [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr process);
}
'@
    $testHandle=[SensorPanelFaultTest]::OpenProcess(1,$false,[uint32]$collector.ProcessId)
    if($testHandle -eq [IntPtr]::Zero){throw 'No se pudo abrir el proceso de prueba.'}
    try {if(-not [SensorPanelFaultTest]::TerminateProcess($testHandle,70)){throw 'No se pudo simular el código del watchdog.'}} finally {[void][SensorPanelFaultTest]::CloseHandle($testHandle)}
    $restartClock=[Diagnostics.Stopwatch]::StartNew()
    $newCollector=$null
    while($restartClock.Elapsed.TotalSeconds -lt 85){Start-Sleep -Seconds 2;$newCollector=Find-PanelProcesses|Where-Object {$_.CommandLine -match '\-\-collector' -and $_.ProcessId -ne $collector.ProcessId}|Select-Object -First 1;if($newCollector){break}}
    if(-not $newCollector -or -not(Get-Process -Id $panel.ProcessId -ErrorAction SilentlyContinue)){throw 'La recuperación automática no se completó.'}
    Set-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Collector' -Trigger $logonTriggers | Out-Null
    [pscustomobject]@{old_collector=$collector.ProcessId;new_collector=$newCollector.ProcessId;panel=$panel.ProcessId;restarted_seconds=$restartClock.Elapsed.TotalSeconds;panel_survived=$true}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $artifactRoot 'recovery-test.json') -Encoding utf8
    Write-Phase 'warming_up' 'Cerrar el editor durante la medición de consumo normal.'
    Start-Sleep -Seconds 25
    Write-Phase 'measuring' 'Muestra de 600 segundos con panel y recolector instalados.'
    & (Join-Path $PSScriptRoot 'Measure.ps1') -ProcessIds @($panel.ProcessId,$newCollector.ProcessId) -Seconds 600 -OutFile (Join-Path $artifactRoot 'performance-installed.json')
    $measurement=Get-Content -LiteralPath (Join-Path $artifactRoot 'performance-installed.json') -Raw | ConvertFrom-Json
    if($measurement.cpu_average_percent -ge 0.2 -or $measurement.private_commit_average_mib -ge 60){throw 'La medición superó el presupuesto inicial. Se restaura AIDA64 hasta ajustar el panel.'}
    if($aida){Export-ScheduledTask -TaskName $aida.TaskName -TaskPath $aida.TaskPath|Set-Content -LiteralPath (Join-Path (Split-Path -Parent $installedExe) 'AIDA64-AutoStart-backup.xml') -Encoding utf8;Disable-ScheduledTask -InputObject $aida | Out-Null}
    $cutover=$true
    Write-Phase 'complete' 'Sensores, recuperación y consumo validados. Inicio anterior de AIDA64 deshabilitado; aplicación y configuración conservadas. Pendientes reinicios y suspensión física.'
} catch {
    Write-Phase 'failed' $_.Exception.Message
    foreach($name in 'Panel','Collector'){Stop-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue;Disable-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue | Out-Null}
    throw
} finally {
    if(-not $cutover -and $aidaWasRunning -and $aida){Start-ScheduledTask -InputObject $aida}
}
