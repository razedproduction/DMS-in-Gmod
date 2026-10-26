# Installs the built module and the Lua addon into Garry's Mod:
#   dist\gmsv_dms_win64.dll -> garrysmod\lua\bin
#   addon\                  -> garrysmod\addons\dms (replaced as a whole)
#
#   .\scripts\install.ps1                          GMod from Steam's default library
#   .\scripts\install.ps1 -GarrysMod "D:\...\GarrysMod"
#   .\scripts\install.ps1 -Fork [-Vphysics <dll>]  also the VPhysics-Box3D fork -> bin\win64\vphysics.dll, from
#                                                  vphysics\build (scripts\build-vphysics.ps1) or -Vphysics; Steam's
#                                                  own is kept once as vphysics.dll.original
#   .\scripts\install.ps1 -Server "E:\Games\GModServer\server" [-Fork [-Vphysics <dll>]]
#                                                  the bench's dedicated server (ROADMAP M2.5). It keeps its own
#                                                  vphysics.dll: the fork doesn't run there yet (the server's x86-64
#                                                  build has a newer IAppSystem and vphysics interfaces, docs\bench.md).
#                                                  -Fork puts the fork in bin\win64 anyway (the server's own kept as
#                                                  vphysics.dll.original)

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

$win64 = Join-Path $target 'bin\win64'
$original = Join-Path $win64 'vphysics.dll.original'

if ($Server -and -not $Fork) {
	if (Test-Path $original) {
		Copy-Item $original (Join-Path $win64 'vphysics.dll') -Force
		Write-Host "-> $win64\vphysics.dll (the server's own, back from vphysics.dll.original)"
	}
}

if ($Fork) {
	if (-not $Vphysics) {
		$Vphysics = Join-Path $root 'vphysics\build\windows-x64\src\Release\vphysics.dll'
		if (-not (Test-Path $Vphysics)) { throw "$Vphysics not built yet, run scripts\build-vphysics.ps1" }
	}
	if (-not (Test-Path $Vphysics)) { throw "$Vphysics not found" }
	# Only the first install keeps what is there: later ones would back up the fork itself.
	if (-not (Test-Path $original)) { Copy-Item (Join-Path $win64 'vphysics.dll') $original }
	Copy-Item $Vphysics (Join-Path $win64 'vphysics.dll') -Force
	Write-Host "-> $win64\vphysics.dll (VPhysics-Box3D from $Vphysics; the original: vphysics.dll.original)"
}
