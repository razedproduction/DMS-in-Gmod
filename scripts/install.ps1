# Installs the built module and the Lua addon into Garry's Mod:
#   dist\gmsv_dms_win64.dll -> garrysmod\lua\bin
#   addon\                  -> garrysmod\addons\dms (replaced as a whole)
#
#   .\scripts\install.ps1                          GMod from Steam's default library
#   .\scripts\install.ps1 -GarrysMod "D:\...\GarrysMod"
#   .\scripts\install.ps1 -Server "E:\Games\GModServer\server" [-Fork [-Vphysics <dll>]]
#                                                  the bench's dedicated server (ROADMAP M2.5). It keeps its own
#                                                  vphysics.dll: the fork doesn't run there yet (the server's x86-64
#                                                  build has a newer IAppSystem and vphysics interfaces, docs\bench.md).
#                                                  -Fork puts the fork in bin\win64 anyway (the server's own kept as
#                                                  vphysics.dll.original), from the game's bin\win64 or -Vphysics

param(
	[string]$GarrysMod,
	[string]$Server,
	[string]$Vphysics,
	[switch]$Fork
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if ($Server) {
	if (-not (Test-Path (Join-Path $Server 'srcds_win64.exe'))) {
		throw "$Server has no srcds_win64.exe (the dedicated server's x86-64 branch, see docs\bench.md)"
	}
	$target = $Server
} else {
	if (-not $GarrysMod) {
		$steam = (Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
		if ($steam) { $GarrysMod = Join-Path $steam 'steamapps\common\GarrysMod' }
	}
	if (-not $GarrysMod -or -not (Test-Path (Join-Path $GarrysMod 'garrysmod'))) {
		throw "Garry's Mod not found, pass -GarrysMod <path to the GarrysMod folder>"
	}
	$target = $GarrysMod
}

$dll = Join-Path $root 'dist\gmsv_dms_win64.dll'
if (-not (Test-Path $dll)) { throw "$dll not built yet, run scripts\build.ps1" }

$bin = Join-Path $target 'garrysmod\lua\bin'
New-Item -ItemType Directory -Force $bin | Out-Null
Copy-Item $dll $bin -Force
Write-Host "-> $bin\gmsv_dms_win64.dll"

$addon = Join-Path $target 'garrysmod\addons\dms'
if (Test-Path $addon) { Remove-Item $addon -Recurse -Force -Confirm:$false }
Copy-Item (Join-Path $root 'addon') $addon -Recurse
Write-Host "-> $addon"

if ($Server -and -not $Fork) {
	$win64 = Join-Path $Server 'bin\win64'
	$original = Join-Path $win64 'vphysics.dll.original'
	if (Test-Path $original) {
		Copy-Item $original (Join-Path $win64 'vphysics.dll') -Force
		Write-Host "-> $win64\vphysics.dll (the server's own, back from vphysics.dll.original)"
	}
}

if ($Server -and $Fork) {
	# The fork as the game has it: its bin\win64 keeps Steam's original beside it as vphysics.dll.original.
	if (-not $Vphysics) {
		$steam = (Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
		$game = if ($GarrysMod) { $GarrysMod } elseif ($steam) { Join-Path $steam 'steamapps\common\GarrysMod' }
		$Vphysics = Join-Path $game 'bin\win64\vphysics.dll'
		if (-not (Test-Path (Join-Path $game 'bin\win64\vphysics.dll.original'))) {
			throw "$Vphysics doesn't look like VPhysics-Box3D (no vphysics.dll.original beside it): pass -Vphysics <the fork's vphysics.dll>"
		}
	}
	if (-not (Test-Path $Vphysics)) { throw "$Vphysics not found" }
	$win64 = Join-Path $Server 'bin\win64'
	$original = Join-Path $win64 'vphysics.dll.original'
	if (-not (Test-Path $original)) { Copy-Item (Join-Path $win64 'vphysics.dll') $original }
	Copy-Item $Vphysics (Join-Path $win64 'vphysics.dll') -Force
	Write-Host "-> $win64\vphysics.dll (VPhysics-Box3D from $Vphysics; the server's own: vphysics.dll.original)"
}
