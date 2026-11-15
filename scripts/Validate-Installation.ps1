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
    if(@(Get-Process -Name aida64 -ErrorAction SilentlyContinue).Count){throw 'AIDA64 is still running; the motherboard test will not start.'}
    $collectorTask=Get-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Collector'
    $logonTriggers=@($collectorTask.Triggers)
    $scheduledProbe=New-ScheduledTaskTrigger -Once -At (Get-Date).AddSeconds(6)
    Set-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Collector' -Trigger @($logonTriggers+$scheduledProbe) | Out-Null
    Start-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Panel'
    Start-Sleep -Seconds 12
    $processes=Find-PanelProcesses
    $collector=$processes|Where-Object {$_.CommandLine -match '\-\-collector'}|Select-Object -First 1
    $panel=$processes|Where-Object {$_.CommandLine -match '\-\-panel'}|Select-Object -First 1
    if(-not $collector -or -not $panel){throw 'Both processes did not start from their scheduled tasks.'}
    $preflight=(@($collector.ProcessId,$panel.ProcessId)|ForEach-Object {(Get-Process -Id $_).PrivateMemorySize64}|Measure-Object -Sum).Sum/1MB
    if($preflight -gt 80){throw "Initial private memory ($([math]::Round($preflight,1)) MiB) requires renderer adjustment before measurement."}
    Write-Phase 'testing_recovery' "Collector $($collector.ProcessId) will be terminated; panel $($panel.ProcessId) must remain running."
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
    if($testHandle -eq [IntPtr]::Zero){throw 'Could not open the probe process.'}
    try {if(-not [SensorPanelFaultTest]::TerminateProcess($testHandle,70)){throw 'Could not simulate the watchdog exit code.'}} finally {[void][SensorPanelFaultTest]::CloseHandle($testHandle)}
    $restartClock=[Diagnostics.Stopwatch]::StartNew()
    $newCollector=$null
    while($restartClock.Elapsed.TotalSeconds -lt 85){Start-Sleep -Seconds 2;$newCollector=Find-PanelProcesses|Where-Object {$_.CommandLine -match '\-\-collector' -and $_.ProcessId -ne $collector.ProcessId}|Select-Object -First 1;if($newCollector){break}}
    if(-not $newCollector -or -not(Get-Process -Id $panel.ProcessId -ErrorAction SilentlyContinue)){throw 'Automatic recovery did not complete.'}
    Set-ScheduledTask -TaskPath '\SensorPanel\' -TaskName 'Collector' -Trigger $logonTriggers | Out-Null
    [pscustomobject]@{old_collector=$collector.ProcessId;new_collector=$newCollector.ProcessId;panel=$panel.ProcessId;restarted_seconds=$restartClock.Elapsed.TotalSeconds;panel_survived=$true}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $artifactRoot 'recovery-test.json') -Encoding utf8
    Write-Phase 'warming_up' 'Keep the editor closed during normal-use resource measurement.'
    Start-Sleep -Seconds 25
    Write-Phase 'measuring' '600-second sample of the installed panel and collector.'
    & (Join-Path $PSScriptRoot 'Measure.ps1') -ProcessIds @($panel.ProcessId,$newCollector.ProcessId) -Seconds 600 -OutFile (Join-Path $artifactRoot 'performance-installed.json')
    $measurement=Get-Content -LiteralPath (Join-Path $artifactRoot 'performance-installed.json') -Raw | ConvertFrom-Json
    if($measurement.cpu_average_percent -ge 0.2 -or $measurement.private_commit_average_mib -ge 60){throw 'The measurement exceeded the initial budget. Restoring AIDA64 until the panel is adjusted.'}
    if($aida){Export-ScheduledTask -TaskName $aida.TaskName -TaskPath $aida.TaskPath|Set-Content -LiteralPath (Join-Path (Split-Path -Parent $installedExe) 'AIDA64-AutoStart-backup.xml') -Encoding utf8;Disable-ScheduledTask -InputObject $aida | Out-Null}
    $cutover=$true
    Write-Phase 'complete' 'Sensors, recovery, and resource use validated. Old AIDA64 startup disabled; application and configuration preserved. Physical reboot and suspend tests remain outstanding.'
} catch {
    Write-Phase 'failed' $_.Exception.Message
    foreach($name in 'Panel','Collector'){Stop-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue;Disable-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue | Out-Null}
    throw
} finally {
    if(-not $cutover -and $aidaWasRunning -and $aida){Start-ScheduledTask -InputObject $aida}
}
