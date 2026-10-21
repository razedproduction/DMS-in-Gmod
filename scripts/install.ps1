# Installs the built module and the Lua addon into Garry's Mod:
#   dist\gmsv_dms_win64.dll -> garrysmod\lua\bin
#   addon\                  -> garrysmod\addons\dms (replaced as a whole)
#
#   .\scripts\install.ps1                          GMod from Steam's default library
#   .\scripts\install.ps1 -GarrysMod "D:\...\GarrysMod"

param(
	[string]$GarrysMod
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if (-not $GarrysMod) {
	$steam = (Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
	if ($steam) { $GarrysMod = Join-Path $steam 'steamapps\common\GarrysMod' }
}
if (-not $GarrysMod -or -not (Test-Path (Join-Path $GarrysMod 'garrysmod'))) {
	throw "Garry's Mod not found, pass -GarrysMod <path to the GarrysMod folder>"
}

$dll = Join-Path $root 'dist\gmsv_dms_win64.dll'
if (-not (Test-Path $dll)) { throw "$dll not built yet, run scripts\build.ps1" }

$bin = Join-Path $GarrysMod 'garrysmod\lua\bin'
New-Item -ItemType Directory -Force $bin | Out-Null
Copy-Item $dll $bin -Force
Write-Host "-> $bin\gmsv_dms_win64.dll"

$addon = Join-Path $GarrysMod 'garrysmod\addons\dms'
if (Test-Path $addon) { Remove-Item $addon -Recurse -Force -Confirm:$false }
Copy-Item (Join-Path $root 'addon') $addon -Recurse
Write-Host "-> $addon"
