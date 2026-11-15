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
    throw 'Abra una Developer PowerShell de Visual Studio con las herramientas C++ y Windows SDK.'
}
if ($Strict -and $CargoArgs[0] -eq 'clippy') {$CargoArgs += @('--','-D','warnings')}
& cargo @CargoArgs
exit $LASTEXITCODE
