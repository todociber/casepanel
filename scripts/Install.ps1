#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [string]$Source=(Split-Path -Parent $PSScriptRoot),
    [string]$UserSid=([Security.Principal.WindowsIdentity]::GetCurrent().User.Value),
    [ValidateRange(0,60)][int]$DelaySeconds=5,
    [switch]$StageOnly,
    [switch]$DisableAidaAutostart
)
$ErrorActionPreference='Stop'
if ($UserSid -notmatch '^S-1-5-21-(\d+-){3}\d+$') {throw 'SID de usuario no admitido.'}
$programFiles=[Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
$installRoot=Join-Path $programFiles 'SensorPanel'
$sourceExe=Join-Path $Source 'sensorpanel.exe'
if (-not(Test-Path -LiteralPath $sourceExe)) {$sourceExe=Join-Path $Source 'target\release\sensorpanel.exe'}
if (-not(Test-Path -LiteralPath $sourceExe)) {throw 'No se encontró sensorpanel.exe. Compile o extraiga el paquete Release.'}
$expectedRoot=[IO.Path]::GetFullPath((Join-Path $programFiles 'SensorPanel'))
if ([IO.Path]::GetFullPath($installRoot) -ne $expectedRoot) {throw 'Directorio de instalación inesperado.'}

$pawnKey='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO'
if (-not(Test-Path -LiteralPath $pawnKey)) {
    $installer=Join-Path $Source 'PawnIO_setup.exe'
    if (-not(Test-Path -LiteralPath $installer)) {$installer=Join-Path $Source '.tools\PawnIO_setup.exe'}
    if (-not(Test-Path -LiteralPath $installer)) {throw 'Falta el instalador oficial PawnIO 2.2.0. Ejecute Prepare-PawnIO.ps1.'}
    if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne '1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032') {throw 'Hash del instalador PawnIO incorrecto.'}
    $signature=Get-AuthenticodeSignature -LiteralPath $installer
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'CN=namazso.eu') {throw 'Firma oficial PawnIO no válida.'}
    $pawn=Start-Process -FilePath $installer -ArgumentList '-install','-silent' -WindowStyle Hidden -PassThru -Wait
    if ($pawn.ExitCode -ne 0 -or -not(Test-Path -LiteralPath $pawnKey)) {throw 'PawnIO no se pudo instalar.'}
}
$pawn=Get-ItemProperty -LiteralPath $pawnKey
if ([version]$pawn.DisplayVersion -lt [version]'2.2.0') {throw 'Se requiere PawnIO 2.2.0 o posterior. Actualice desde la distribución oficial.'}

New-Item -ItemType Directory -Path $installRoot -Force | Out-Null
$acl=New-Object Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true,$false)
$acl.SetOwner((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')))
foreach($entry in @(@('S-1-5-18','FullControl'),@('S-1-5-32-544','FullControl'),@('S-1-5-32-545','ReadAndExecute'))) {
    $sid=New-Object Security.Principal.SecurityIdentifier($entry[0])
    $rule=New-Object Security.AccessControl.FileSystemAccessRule($sid,$entry[1],'ContainerInherit,ObjectInherit','None','Allow')
    $acl.AddAccessRule($rule)
}
Set-Acl -LiteralPath $installRoot -AclObject $acl
$scheduler=New-Object -ComObject 'Schedule.Service';$scheduler.Connect()
try {$null=$scheduler.GetFolder('\SensorPanel')} catch {$null=$scheduler.GetFolder('\').CreateFolder('SensorPanel')}
$installedExe=Join-Path $installRoot 'sensorpanel.exe'
$runningTasks=@(Get-ScheduledTask -TaskPath '\SensorPanel\' -ErrorAction SilentlyContinue | Where-Object {$_.TaskName -in @('Panel','Collector') -and $_.State -eq 'Running'})
$oldProcesses=@(Get-CimInstance Win32_Process -Filter "Name='sensorpanel.exe'" | Where-Object {$_.ExecutablePath -eq $installedExe})
foreach($name in 'Panel','Collector') {Stop-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $name -ErrorAction SilentlyContinue}
try {
    # Stopping a task returns before its process necessarily releases the image.
    foreach($oldProcess in $oldProcesses) {
        $process=Get-Process -Id $oldProcess.ProcessId -ErrorAction SilentlyContinue
        if($process -and -not $process.WaitForExit(15000)) {throw 'SensorPanel no terminó en 15 segundos; se cancela la sustitución.'}
    }
    for($copyAttempt=0;$copyAttempt -lt 20;$copyAttempt++) {
        try {Copy-Item -LiteralPath $sourceExe -Destination $installedExe -Force;break}
        catch [IO.IOException] {if($copyAttempt -eq 19){throw};Start-Sleep -Milliseconds 250}
    }
} catch {
    foreach($oldTask in $runningTasks) {Start-ScheduledTask -InputObject $oldTask -ErrorAction SilentlyContinue}
    throw
}
$sourceHash=(Get-FileHash -LiteralPath $sourceExe -Algorithm SHA256).Hash
if ((Get-FileHash -LiteralPath (Join-Path $installRoot 'sensorpanel.exe')) -ne $null) {
    if ((Get-FileHash -LiteralPath (Join-Path $installRoot 'sensorpanel.exe') -Algorithm SHA256).Hash -ne $sourceHash) {throw 'Copia del ejecutable inválida.'}
}
foreach($name in 'README.md','THIRD-PARTY-NOTICES.md','LICENSE') {if(Test-Path -LiteralPath (Join-Path $Source $name)){Copy-Item -LiteralPath (Join-Path $Source $name) -Destination $installRoot -Force}}
$sourceDocs=Join-Path $Source 'docs'
if(Test-Path -LiteralPath $sourceDocs){Copy-Item -LiteralPath $sourceDocs -Destination $installRoot -Recurse -Force}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Uninstall.ps1') -Destination $installRoot -Force
$settings=New-ScheduledTaskSettingsSet -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable
$settings.UseUnifiedSchedulingEngine=$false
foreach($mode in @(@('Panel','--panel','Limited'),@('Collector','--collector','Highest'))) {
    $trigger=New-ScheduledTaskTrigger -AtLogOn -User $UserSid
    $trigger.Delay="PT${DelaySeconds}S"
    $principal=New-ScheduledTaskPrincipal -UserId $UserSid -LogonType Interactive -RunLevel $mode[2]
    $action=New-ScheduledTaskAction -Execute (Join-Path $installRoot 'sensorpanel.exe') -Argument $mode[1] -WorkingDirectory $installRoot
    $modeSettings=$settings
    if($mode[0] -eq 'Collector'){$modeSettings=New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable}
    Register-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $mode[0] -Action $action -Principal $principal -Trigger $trigger -Settings $modeSettings -Description 'SensorPanel nativo, sin comprobación automática de actualizaciones.' -Force | Out-Null
    if($StageOnly){Disable-ScheduledTask -TaskPath '\SensorPanel\' -TaskName $mode[0] | Out-Null}
}
if($DisableAidaAutostart){
    $old=Get-ScheduledTask -TaskName 'AIDA64 AutoStart' -ErrorAction SilentlyContinue
    if($old){Export-ScheduledTask -TaskName $old.TaskName -TaskPath $old.TaskPath | Set-Content -LiteralPath (Join-Path $installRoot 'AIDA64-AutoStart-backup.xml') -Encoding utf8;Disable-ScheduledTask -InputObject $old | Out-Null;Stop-ScheduledTask -InputObject $old}
}
[pscustomobject]@{version='0.1.0';exe_sha256=$sourceHash;user_sid=$UserSid;delay_seconds=$DelaySeconds;staged=[bool]$StageOnly;pawnio=$pawn.DisplayVersion;installed_at=(Get-Date).ToUniversalTime().ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $installRoot 'installation.json') -Encoding utf8
Write-Output "Instalado en $installRoot"
Write-Output 'Tareas: \SensorPanel\Panel y \SensorPanel\Collector. No se modificó la tarea de uso de IA.'
