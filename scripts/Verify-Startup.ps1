[CmdletBinding()]
param([string]$OutFile=(Join-Path $env:LOCALAPPDATA 'SensorPanel\startup-verification.json'))
$ErrorActionPreference='Stop'
$os=Get-CimInstance Win32_OperatingSystem
$boot=$os.LastBootUpTime.ToUniversalTime().ToString('o')
$checks=@()
if(Test-Path -LiteralPath $OutFile){$checks=@(Get-Content -LiteralPath $OutFile -Raw | ConvertFrom-Json)}
$tasks=@(foreach($name in 'Panel','Collector'){$task=Get-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name;$info=Get-ScheduledTaskInfo -InputObject $task;[pscustomobject]@{name=$name;state=$task.State.ToString();last_run=$info.LastRunTime.ToUniversalTime().ToString('o');last_result=$info.LastTaskResult;started_this_boot=($info.LastRunTime -ge $os.LastBootUpTime)}})
$checks=@($checks | Where-Object {$_.boot -ne $boot})
$checks += [pscustomobject]@{boot=$boot;checked=(Get-Date).ToUniversalTime().ToString('o');tasks=$tasks;passed=(@($tasks | Where-Object {$_.state -ne 'Running' -or -not $_.started_this_boot}).Count -eq 0)}
New-Item -ItemType Directory -Path (Split-Path -Parent $OutFile) -Force | Out-Null
ConvertTo-Json -InputObject @($checks) -Depth 6 | Set-Content -LiteralPath $OutFile -Encoding utf8
[pscustomobject]@{different_boots=$checks.Count;successful_boots=@($checks|Where-Object passed).Count;checks=$checks} | ConvertTo-Json -Depth 7
