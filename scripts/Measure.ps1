[CmdletBinding()]
param([Parameter(Mandatory=$true)][int[]]$ProcessIds,[ValidateRange(5,3600)][int]$Seconds=600,[string]$OutFile=(Join-Path (Split-Path -Parent $PSScriptRoot) 'artifacts\performance.json'))
$ErrorActionPreference='Stop'
$cores=[Environment]::ProcessorCount
$samples=[Collections.Generic.List[object]]::new()
$previous=@{}
$clock=[Diagnostics.Stopwatch]::StartNew()
foreach($processId in $ProcessIds){$p=Get-Process -Id $processId -ErrorAction Stop;$previous[$processId]=[pscustomobject]@{cpu=$p.TotalProcessorTime.TotalSeconds;at=0.0}}
for($second=1;$second -le $Seconds;$second++) {
    Start-Sleep -Seconds 1
    $items=@()
    foreach($processId in $ProcessIds){
        $p=Get-Process -Id $processId -ErrorAction SilentlyContinue
        if(-not $p){throw "Process $processId exited before the measurement completed."}
        $at=$clock.Elapsed.TotalSeconds;$cpu=$p.TotalProcessorTime.TotalSeconds;$old=$previous[$processId]
        $items += [pscustomobject]@{id=$processId;cpu_percent=100*($cpu-$old.cpu)/($at-$old.at)/$cores;resident_mib=$p.WorkingSet64/1MB;private_commit_mib=$p.PrivateMemorySize64/1MB}
        $previous[$processId]=[pscustomobject]@{cpu=$cpu;at=$at}
    }
    $samples.Add([pscustomobject]@{second=$second;cpu_percent=($items.cpu_percent | Measure-Object -Sum).Sum;resident_mib=($items.resident_mib | Measure-Object -Sum).Sum;private_commit_mib=($items.private_commit_mib | Measure-Object -Sum).Sum;processes=$items})
}
$report=[pscustomobject]@{duration_seconds=$clock.Elapsed.TotalSeconds;logical_processors=$cores;process_ids=$ProcessIds;cpu_average_percent=($samples.cpu_percent|Measure-Object -Average).Average;resident_average_mib=($samples.resident_mib|Measure-Object -Average).Average;private_commit_average_mib=($samples.private_commit_mib|Measure-Object -Average).Average;private_commit_min_mib=($samples.private_commit_mib|Measure-Object -Minimum).Minimum;private_commit_max_mib=($samples.private_commit_mib|Measure-Object -Maximum).Maximum;samples=$samples}
New-Item -ItemType Directory -Path (Split-Path -Parent $OutFile) -Force | Out-Null
$report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $OutFile -Encoding utf8
$report | Select-Object -ExcludeProperty samples | ConvertTo-Json
