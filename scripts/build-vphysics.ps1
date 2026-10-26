# Builds the VPhysics-Box3D fork (vphysics\, docs\vphysics.md) for GMod's x86-64 branch:
# vphysics\build\windows-x64\src\Release\vphysics.dll.
#
#   .\scripts\build-vphysics.ps1              release
#   .\scripts\build-vphysics.ps1 -DebugBuild
#   .\scripts\build-vphysics.ps1 -Clean       rebuilds everything (after a subtree pull or switching commits: an
#                                             incremental build across them has given a broken dll)

param(
	[switch]$DebugBuild,
	[switch]$Clean
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$vphysics = Join-Path $root 'vphysics'
$config = if ($DebugBuild) { 'debug' } else { 'release' }

Push-Location $root
try {
	# Box3D and the Source SDK are submodules of this repository (vphysics\external\*).
	& git submodule update --init --depth 1 vphysics/external/box3d vphysics/external/source-sdk-2013
	if ($LASTEXITCODE -ne 0) { throw "git submodule update failed ($LASTEXITCODE)" }
} finally {
	Pop-Location
}

Push-Location $vphysics
try {
	& cmake --preset windows-x64
	if ($LASTEXITCODE -ne 0) { throw "cmake configure failed ($LASTEXITCODE)" }
	$buildArgs = @('--build', '--preset', "windows-x64-$config")
	if ($Clean) { $buildArgs += '--clean-first' }
	& cmake @buildArgs
	if ($LASTEXITCODE -ne 0) { throw "cmake build failed ($LASTEXITCODE)" }
} finally {
	Pop-Location
}

$dir = if ($DebugBuild) { 'Debug' } else { 'Release' }
Write-Host "-> $(Join-Path $vphysics "build\windows-x64\src\$dir\vphysics.dll")"
