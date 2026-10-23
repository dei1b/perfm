param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\dist\Perfm-Integrated')
)

$ErrorActionPreference = 'Stop'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [IO.Path]::GetFullPath($OutputDirectory)
$releaseExe = Join-Path $root 'target\release\perfm.exe'
$bridgePublish = Join-Path $root 'target\bridge-publish'
$bridgeOutput = Join-Path $output 'SensorBridge'
$privateDotnet = Join-Path $root 'target\dotnet\dotnet.exe'
$dotnet = if (Test-Path -LiteralPath $privateDotnet) { $privateDotnet } else { 'dotnet' }

if (-not (Test-Path -LiteralPath $releaseExe -PathType Leaf)) {
    throw 'Build the Rust executable first: cargo build --release'
}

& $dotnet publish (Join-Path $root 'bridge\Perfm.SensorBridge.csproj') -c Release -r win-x64 --self-contained false -o $bridgePublish
if ($LASTEXITCODE -ne 0) {
    throw 'The sensor bridge build failed.'
}

New-Item -ItemType Directory -Force -Path $bridgeOutput | Out-Null
Copy-Item -Path (Join-Path $bridgePublish '*') -Destination $bridgeOutput -Recurse -Force
Copy-Item -LiteralPath $releaseExe -Destination (Join-Path $output 'Perfm.exe') -Force
Copy-Item -LiteralPath (Join-Path $root 'scripts\Run Perfm with CPU sensors.cmd') -Destination (Join-Path $output 'Run Perfm with CPU sensors.cmd') -Force
Copy-Item -LiteralPath (Join-Path $root 'licenses\LibreHardwareMonitor-MPL-2.0.txt') -Destination (Join-Path $bridgeOutput 'LibreHardwareMonitor-MPL-2.0.txt') -Force
Copy-Item -LiteralPath (Join-Path $root 'licenses\LibreHardwareMonitor-THIRD-PARTY-NOTICES.txt') -Destination (Join-Path $bridgeOutput 'LibreHardwareMonitor-THIRD-PARTY-NOTICES.txt') -Force

Write-Output "Packaged Perfm and LibreHardwareMonitorLib 0.9.6 in $output"
