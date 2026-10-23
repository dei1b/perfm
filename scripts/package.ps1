param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\dist\Perfm-Integrated'),
    [ValidateSet('NativeAot', 'Managed')]
    [string]$BridgeMode = 'NativeAot'
)

$ErrorActionPreference = 'Stop'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [IO.Path]::GetFullPath($OutputDirectory)
$releaseExe = Join-Path $root 'target\release\perfm.exe'
$bridgePublish = Join-Path $root "target\bridge-publish-$($BridgeMode.ToLowerInvariant())"
$bridgeOutput = Join-Path $output 'SensorBridge'
$privateDotnet = Join-Path $root 'target\dotnet\dotnet.exe'
$dotnet = if (Test-Path -LiteralPath $privateDotnet) { $privateDotnet } else { 'dotnet' }

if (-not (Test-Path -LiteralPath $releaseExe -PathType Leaf)) {
    throw 'Build the Rust executable first: cargo build --release'
}

$publishArguments = @('publish', (Join-Path $root 'bridge\Perfm.SensorBridge.csproj'), '-c', 'Release', '-r', 'win-x64', '-o', $bridgePublish)
if ($BridgeMode -eq 'NativeAot') {
    $publishArguments += @('--self-contained', 'true', '-p:PublishAot=true', '-p:PublishTrimmed=true', '-p:DebugType=none')
} else {
    $publishArguments += @('--self-contained', 'false')
}
& $dotnet @publishArguments
if ($LASTEXITCODE -ne 0) {
    throw 'The sensor bridge build failed.'
}

New-Item -ItemType Directory -Force -Path $bridgeOutput | Out-Null
Get-ChildItem -LiteralPath $bridgeOutput -File |
    Where-Object { $_.Extension -in '.exe', '.dll', '.json', '.pdb' } |
    Remove-Item -Force
Get-ChildItem -LiteralPath $bridgePublish -File | Where-Object Extension -ne '.pdb' | Copy-Item -Destination $bridgeOutput -Force
Copy-Item -LiteralPath $releaseExe -Destination (Join-Path $output 'Perfm.exe') -Force
Copy-Item -LiteralPath (Join-Path $root 'scripts\Run Perfm with CPU sensors.cmd') -Destination (Join-Path $output 'Run Perfm with CPU sensors.cmd') -Force
Copy-Item -LiteralPath (Join-Path $root 'licenses\LibreHardwareMonitor-MPL-2.0.txt') -Destination (Join-Path $bridgeOutput 'LibreHardwareMonitor-MPL-2.0.txt') -Force
Copy-Item -LiteralPath (Join-Path $root 'licenses\LibreHardwareMonitor-THIRD-PARTY-NOTICES.txt') -Destination (Join-Path $bridgeOutput 'LibreHardwareMonitor-THIRD-PARTY-NOTICES.txt') -Force

Write-Output "Packaged Perfm and LibreHardwareMonitorLib 0.9.6 ($BridgeMode bridge) in $output"
