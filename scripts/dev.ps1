param([switch]$Strict,[Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArgs)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
if (Test-Path -LiteralPath "$projectRoot\.tools\cargo\bin\cargo.exe") {
    $env:RUSTUP_HOME="$projectRoot\.tools\rustup"
    $env:CARGO_HOME="$projectRoot\.tools\cargo"
    $env:PATH="$projectRoot\.tools\cargo\bin;$env:PATH"
}
$localMsvc="$projectRoot\.tools\msvc\Contents\VC\Tools\MSVC\14.44.35207"
$sdkRoot='C:\Program Files (x86)\Windows Kits\10'
$sdkVersion='10.0.26100.0'
if (Test-Path -LiteralPath "$localMsvc\bin\Hostx64\x64\link.exe") {
    $env:PATH="$localMsvc\bin\Hostx64\x64;$sdkRoot\bin\$sdkVersion\x64;$env:PATH"
    $env:LIB="$localMsvc\lib\x64;$localMsvc\lib\onecore\x64;$sdkRoot\Lib\$sdkVersion\um\x64;$sdkRoot\Lib\$sdkVersion\ucrt\x64"
} elseif (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $vsInstall=$null
    if(Test-Path -LiteralPath $vswhere){$vsInstall=& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath}
    if(-not $vsInstall){throw 'Install Visual Studio C++ build tools and a Windows SDK, or use a Developer PowerShell.'}
    & (Join-Path $vsInstall 'Common7\Tools\Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
}
if ($Strict -and $CargoArgs[0] -eq 'clippy') {$CargoArgs += @('--','-D','warnings')}
& cargo @CargoArgs
exit $LASTEXITCODE
