# Builds gmsv_dms for GMod's x86-64 branch into dist\gmsv_dms_win64.dll.
#
#   .\scripts\build.ps1            release
#   .\scripts\build.ps1 -DebugBuild

param(
	[switch]$DebugBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$target = 'x86_64-pc-windows-msvc'
$profileDir = if ($DebugBuild) { 'debug' } else { 'release' }

$cargoArgs = @('build', '-p', 'gmsv_dms', '--target', $target)
if (-not $DebugBuild) { $cargoArgs += '--release' }

Push-Location $root
try {
	& cargo @cargoArgs
	if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
} finally {
	Pop-Location
}

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$out = Join-Path $dist 'gmsv_dms_win64.dll'
Copy-Item (Join-Path $root "target\$target\$profileDir\gmsv_dms.dll") $out -Force
Write-Host "-> $out"
