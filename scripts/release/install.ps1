# DMS installer for players: run it from the unpacked release (DMS-<version>-win64\), Garry's Mod closed.
#
#   .\install.ps1 -Gta "D:\Games\Grand Theft Auto IV"   install, and make Homer's model from your GTA IV
#   .\install.ps1                                       install only (the model and the GTA IV folder stay as they are)
#   .\install.ps1 -Model -Gta "D:\...\GTA IV"            only make Homer's model again
#   .\install.ps1 -Uninstall                            Steam's vphysics.dll back, DMS's files out
#   -GarrysMod "D:\...\GarrysMod"                       when Garry's Mod isn't in Steam's default library
#
# What goes where (docs\install.md):
#   bin\vphysics.dll         -> GarrysMod\bin\win64\vphysics.dll (Steam's own kept once as vphysics.dll.original)
#   bin\gmsv_dms_win64.dll   -> GarrysMod\garrysmod\lua\bin
#   addon\                   -> GarrysMod\garrysmod\addons\dms
#   -Gta                     -> GarrysMod\garrysmod\data\dms\game_dir.txt (the folder's path only), then
#                               bin\dms-homer.exe makes garrysmod\models\dms\homer.mdl from your copy of the game
# Nothing of GTA IV is copied anywhere, and nothing leaves your machine.

param(
	[string]$GarrysMod,
	[string]$Gta,
	[switch]$Model,
	[switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot

if (-not $GarrysMod) {
	$steam = (Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
	if ($steam) {
		$GarrysMod = Join-Path $steam 'steamapps\common\GarrysMod'
		# Other Steam libraries.
		if (-not (Test-Path (Join-Path $GarrysMod 'garrysmod'))) {
			$vdf = Join-Path $steam 'steamapps\libraryfolders.vdf'
			if (Test-Path $vdf) {
				foreach ($m in [regex]::Matches((Get-Content $vdf -Raw), '"path"\s+"([^"]+)"')) {
					$g = Join-Path ($m.Groups[1].Value -replace '\\\\', '\') 'steamapps\common\GarrysMod'
					if (Test-Path (Join-Path $g 'garrysmod')) { $GarrysMod = $g; break }
				}
			}
		}
	}
}
if (-not $GarrysMod -or -not (Test-Path (Join-Path $GarrysMod 'garrysmod'))) {
	throw "Garry's Mod not found: pass -GarrysMod <the GarrysMod folder>"
}
$win64 = Join-Path $GarrysMod 'bin\win64'
if (-not (Test-Path (Join-Path $win64 'vphysics.dll'))) {
	throw "$win64\vphysics.dll isn't there: DMS needs Garry's Mod's x86-64 branch (Steam: Garry's Mod -> Properties -> Betas -> x86-64)"
}
if (Get-Process -Name 'gmod*', 'hl2' -ErrorAction SilentlyContinue) {
	throw "Garry's Mod is running: close it first (its vphysics.dll can't be replaced while it runs)"
}
$gameData = Join-Path $GarrysMod 'garrysmod'
$original = Join-Path $win64 'vphysics.dll.original'

if ($Uninstall) {
	if (Test-Path $original) {
		Copy-Item $original (Join-Path $win64 'vphysics.dll') -Force
		Remove-Item $original -Confirm:$false
		Write-Host "-> $win64\vphysics.dll: Steam's own again"
	} else {
		Write-Host "No vphysics.dll.original: leaving vphysics.dll (Steam -> Verify integrity of game files puts its own back)"
	}
	foreach ($p in (Join-Path $gameData 'lua\bin\gmsv_dms_win64.dll'), (Join-Path $gameData 'addons\dms')) {
		if (Test-Path $p) {
			Remove-Item $p -Recurse -Force -Confirm:$false
			Write-Host "removed $p"
		}
	}
	Write-Host "Kept (yours, made from your GTA IV): garrysmod\models\dms, garrysmod\materials\models\dms, garrysmod\data\dms"
	return
}

if (-not $Model) {
	# Only the first install keeps what is there: later ones would back up the fork itself.
	if (-not (Test-Path $original)) { Copy-Item (Join-Path $win64 'vphysics.dll') $original }
	Copy-Item (Join-Path $here 'bin\vphysics.dll') (Join-Path $win64 'vphysics.dll') -Force
	Write-Host "-> $win64\vphysics.dll (VPhysics-Box3D with IPhysicsNM; Steam's own: vphysics.dll.original)"

	$bin = Join-Path $gameData 'lua\bin'
	New-Item -ItemType Directory -Force $bin | Out-Null
	Copy-Item (Join-Path $here 'bin\gmsv_dms_win64.dll') $bin -Force
	Write-Host "-> $bin\gmsv_dms_win64.dll"

	$addon = Join-Path $gameData 'addons\dms'
	if (Test-Path $addon) { Remove-Item $addon -Recurse -Force -Confirm:$false }
	Copy-Item (Join-Path $here 'addon') $addon -Recurse
	Write-Host "-> $addon"
}

if ($Gta) {
	if (-not (Test-Path (Join-Path $Gta 'GTAIV.exe')) -and -not (Test-Path (Join-Path $Gta 'default.xex'))) {
		throw "$Gta isn't a GTA IV folder (PC: with GTAIV.exe; Xbox 360: with default.xex)"
	}
	$data = Join-Path $gameData 'data\dms'
	New-Item -ItemType Directory -Force $data | Out-Null
	[IO.File]::WriteAllText((Join-Path $data 'game_dir.txt'), (Resolve-Path $Gta).Path)
	Write-Host "-> $data\game_dir.txt (DMS reads NM's data from $Gta; dms_game_dir overrides it)"
	& (Join-Path $here 'bin\dms-homer.exe') --game $Gta --gmod $GarrysMod
	if ($LASTEXITCODE -ne 0) { throw "dms-homer failed ($LASTEXITCODE)" }
} elseif ($Model) {
	throw "-Model needs -Gta <your GTA IV folder>"
} elseif (-not (Test-Path (Join-Path $gameData 'models\dms\homer.mdl'))) {
	Write-Host "Homer's model isn't made yet: run .\install.ps1 -Model -Gta <your GTA IV folder>"
}

Write-Host "Done. In Garry's Mod (a local game, x86-64 branch): dms_homer in the console spawns Homer, dms_menu opens the DMS window."
