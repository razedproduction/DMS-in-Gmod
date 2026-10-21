# The bench (ROADMAP M2.5, docs\bench.md): Garry's Mod without a window for tools\dms-ctl.
#
#   .\scripts\server.ps1 start [-Map gm_flatgrass] [-GtaDir <GTA IV dir>] [-NoInstall]
#   .\scripts\server.ps1 stop | restart | status
#
# By default it's the game itself (gmod_win64.exe -textmode, a listen server on 127.0.0.1): the only place
# VPhysics-Box3D runs (the x86-64 dedicated server is built with newer vphysics interfaces than the fork, docs\bench.md).
# -Dedicated [-Server <dir>] starts the dedicated server instead, with its own vphysics.dll (no IPhysicsNM: the module's
# channel, the bench agent and the records work, the drives don't); update [-Server <dir>] [-SteamCmd <exe>] installs or
# updates it (SteamCMD: app 4020, branch x86-64).
#
# start installs dist\ and addon\ (install.ps1), writes garrysmod\data\dms\bench.txt (dms_game_dir, dms_dev 1 and a new
# token; the addon reads and deletes it) and starts the game hidden, RCON on 127.0.0.1 with a new password, the console
# logged to garrysmod\console.log. What dms-ctl needs goes to .bench\server.txt (not in git).
# The game can't run twice: don't start the bench while playing, stop it before playing.

param(
	[Parameter(Position = 0)][ValidateSet('start', 'stop', 'restart', 'status', 'update')][string]$Action = 'status',
	[switch]$Dedicated,
	[string]$GarrysMod,
	[string]$Server,
	[string]$Map = 'gm_flatgrass',
	[string]$GtaDir,
	[int]$Port = 27115,
	[int]$DevPort = 27099,
	[string]$SteamCmd,
	[int]$Wait = 180,
	[switch]$NoInstall
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$benchDir = Join-Path $root '.bench'
$stateFile = Join-Path $benchDir 'server.txt'
$dirFile = Join-Path $benchDir 'server_dir.txt'
New-Item -ItemType Directory -Force $benchDir | Out-Null

function Read-State {
	$s = @{}
	if (Test-Path $stateFile) {
		foreach ($l in Get-Content $stateFile) {
			$k, $v = $l -split '=', 2
			if ($v -ne $null) { $s[$k.Trim()] = $v.Trim() }
		}
	}
	$s
}

function Get-GameDir {
	if ($GarrysMod) { return $GarrysMod }
	$steam = (Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
	if ($steam) { return (Join-Path $steam 'steamapps\common\GarrysMod') }
	throw "Garry's Mod not found, pass -GarrysMod <path to the GarrysMod folder>"
}

function Get-ServerDir {
	if ($Server) {
		Set-Content $dirFile $Server -Encoding utf8
		return $Server
	}
	if (Test-Path $dirFile) { return (Get-Content $dirFile -Raw).Trim() }
	throw "where is the dedicated server? pass -Server <dir with srcds_win64.exe> once (it's remembered in .bench\server_dir.txt)"
}

function Get-Running($s) {
	if (-not $s.pid) { return $null }
	$p = Get-Process -Id $s.pid -ErrorAction SilentlyContinue
	if ($p -and ($p.ProcessName -like 'srcds*' -or $p.ProcessName -like 'gmod*')) { return $p }
	return $null
}

function New-Secret {
	$bytes = New-Object byte[] 16
	[System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
	-join ($bytes | ForEach-Object { $_.ToString('x2') })
}

function Hide-Window([IntPtr]$h) {
	if (-not ('DmsBench.Win' -as [type])) {
		Add-Type -Name Win -Namespace DmsBench -MemberDefinition '[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);'
	}
	[DmsBench.Win]::ShowWindow($h, 0) | Out-Null
	$true
}

function Test-Port([int]$p) {
	$c = New-Object System.Net.Sockets.TcpClient
	try {
		$t = $c.ConnectAsync('127.0.0.1', $p)
		return $t.Wait(300) -and $c.Connected
	} catch { return $false } finally { $c.Close() }
}

function Show-Status {
	$s = Read-State
	$p = Get-Running $s
	if (-not $p) {
		Write-Host "bench: not running"
		return
	}
	$rcon = if (Test-Port ([int]$s.rcon_port)) { 'up' } else { 'down' }
	$dev = if (Test-Port ([int]$s.dev_port)) { 'up' } else { 'down' }
	Write-Host ("bench: running ({0}), pid {1}, {2}; RCON 127.0.0.1:{3} {4}; dms-ctl channel 127.0.0.1:{5} {6}" -f `
		$s.kind, $p.Id, $s.dir, $s.rcon_port, $rcon, $s.dev_port, $dev)
	Write-Host "log: $($s.log)"
}

function Stop-Bench {
	$s = Read-State
	$p = Get-Running $s
	if (-not $p) {
		Write-Host "bench: not running"
	} else {
		$ctl = Join-Path $root 'target\release\dms-ctl.exe'
		if (Test-Path $ctl) { try { & $ctl exec quit 2>$null | Out-Null } catch {} }
		if (-not $p.WaitForExit(10000)) { Stop-Process -Id $p.Id -Force -Confirm:$false }
		Write-Host "bench: stopped (pid $($p.Id))"
	}
	if ($s.dir) {
		$settings = Join-Path $s.dir 'garrysmod\data\dms\bench.txt'
		if (Test-Path $settings) { Remove-Item $settings -Force -Confirm:$false }
	}
	if (Test-Path $stateFile) { Remove-Item $stateFile -Force -Confirm:$false }
}

function Update-Server {
	$dir = Get-ServerDir
	if (-not $SteamCmd) { $SteamCmd = Join-Path (Split-Path -Parent $dir) 'steamcmd\steamcmd.exe' }
	if (-not (Test-Path $SteamCmd)) { throw "$SteamCmd not found, pass -SteamCmd <steamcmd.exe>" }
	# SteamCMD often fails its first run after updating itself (7) or with "state is 0x2" (8): try again.
	for ($i = 1; $i -le 3; $i++) {
		& $SteamCmd +force_install_dir $dir +login anonymous +app_update 4020 -beta x86-64 +quit
		if ($LASTEXITCODE -eq 0) { break }
		Write-Host "steamcmd exited with $LASTEXITCODE, again ($i/3)"
	}
	if ($LASTEXITCODE -ne 0) { throw "steamcmd failed ($LASTEXITCODE)" }
	Write-Host "updated $dir"
}

function Start-Bench {
	$s = Read-State
	if (Get-Running $s) {
		Write-Host "bench: already running (pid $($s.pid)); restart to start it anew"
		return
	}
	if ($Dedicated) {
		$dir = Get-ServerDir
		$exe = Join-Path $dir 'srcds_win64.exe'
		if (-not (Test-Path $exe)) { throw "$exe not found (scripts\server.ps1 update -Server <dir>)" }
		if (-not $NoInstall) { & (Join-Path $PSScriptRoot 'install.ps1') -Server $dir }
	} else {
		$dir = Get-GameDir
		$exe = Join-Path $dir 'gmod_win64.exe'
		if (-not (Test-Path $exe)) { throw "$exe not found (the x86-64 branch of Garry's Mod)" }
		if (Get-Process gmod*, hl2 -ErrorAction SilentlyContinue) { throw "Garry's Mod is running: close it first (it can't run twice)" }
		if (-not $NoInstall) { & (Join-Path $PSScriptRoot 'install.ps1') -GarrysMod $dir }
	}

	if (-not $GtaDir) { $GtaDir = $env:DMS_GAME_DIR }
	$gameFile = Join-Path $root 'golden\game_dir.txt'
	if (-not $GtaDir -and (Test-Path $gameFile)) { $GtaDir = (Get-Content $gameFile -Raw).Trim() }
	if (-not $GtaDir) { Write-Host "no GTA IV folder (-GtaDir, DMS_GAME_DIR or golden\game_dir.txt): NM's data won't load" }

	$password = New-Secret
	$token = New-Secret
	$data = Join-Path $dir 'garrysmod\data\dms'
	New-Item -ItemType Directory -Force $data | Out-Null
	[System.IO.File]::WriteAllLines((Join-Path $data 'bench.txt'), [string[]]@(
		"dms_game_dir `"$GtaDir`""
		"dms_dev_port $DevPort"
		"dms_dev_token $token"
		"dms_dev 1"
	))
	$log = Join-Path $dir 'garrysmod\console.log'
	if (Test-Path $log) { Remove-Item $log -Force -Confirm:$false }

	$common = @('-nocrashdialog', '-condebug', '-ip', '127.0.0.1', '-port', $Port, '+sv_lan', '1', '+sv_cheats', '1',
		'+rcon_password', $password)
	if ($Dedicated) {
		$launch = @('-console', '-game', 'garrysmod') + $common + @('+maxplayers', '1', '+sv_hibernate_think', '1', '+map', $Map)
	} else {
		# A listen server (2 slots: maxplayers 1 would be single player, no RCON); -usercon opens RCON there.
		$launch = @('-textmode', '-nosound', '-novid', '-windowed', '-w', '640', '-h', '480', '-usercon') +
			$common + @('+maxplayers', '2', '+sv_hibernate_think', '1', '+map', $Map)
	}
	$p = Start-Process -FilePath $exe -ArgumentList $launch -WorkingDirectory $dir -WindowStyle Hidden -PassThru
	[System.IO.File]::WriteAllLines($stateFile, [string[]]@(
		"kind=$(if ($Dedicated) { 'dedicated' } else { 'game' })"
		"dir=$dir"
		"pid=$($p.Id)"
		"rcon_port=$Port"
		"rcon_password=$password"
		"dev_port=$DevPort"
		"dev_token=$token"
		"log=$log"
		"map=$Map"
	))
	Write-Host "bench: starting $(Split-Path -Leaf $exe), pid $($p.Id), $Map, 127.0.0.1:$Port; waiting for the dms-ctl channel..."
	$deadline = (Get-Date).AddSeconds($Wait)
	$hidden = $Dedicated
	while ((Get-Date) -lt $deadline) {
		if ($p.HasExited) { throw "the bench exited ($($p.ExitCode)): see $log" }
		# The game opens its window even with -textmode and a hidden start: hide it once it's there.
		if (-not $hidden) {
			$p.Refresh()
			if ($p.MainWindowHandle -ne [IntPtr]::Zero) { $hidden = Hide-Window $p.MainWindowHandle }
		}
		if ($hidden -and (Test-Port $DevPort)) {
			Show-Status
			return
		}
		Start-Sleep -Milliseconds 500
	}
	Write-Host "the dms-ctl channel didn't come up in $Wait s (see $log); RCON: $(if (Test-Port $Port) { 'up' } else { 'down' })"
}

switch ($Action) {
	'start' { Start-Bench }
	'stop' { Stop-Bench }
	'restart' { Stop-Bench; Start-Bench }
	'status' { Show-Status }
	'update' { Update-Server }
}
