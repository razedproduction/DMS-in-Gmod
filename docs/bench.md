# The bench: GMod + `dms-ctl` (ROADMAP M2.5)

The bench plays the role of the Playroom's `--headless`/`--script --rec`, but on Source + Box3D: Claude starts Garry's
Mod on its own, drives it and sees its state in real time; what's left to the user is the visual check. The decisions:
[ADR 0012](adr/0012-bench-game-and-dms-ctl.md).

## In one command

```powershell
.\scripts\build.ps1                     # the module -> dist\
cargo build --release -p dms-ctl -p golden
.\scripts\server.ps1 start              # the game in a window, the dms-ctl channel and RCON on 127.0.0.1
.\target\release\dms-ctl.exe exec status
.\target\release\dms-ctl.exe lua "return DMS.Version"
.\target\release\dms-ctl.exe watch --for 2s
.\target\release\dms-ctl.exe run golden\scenarios\stand-1s.txt
.\target\release\golden.exe behaviour --source
.\scripts\server.ps1 stop
```

## `scripts\server.ps1`

- `start` installs `dist\` and `addon\` into the game (`install.ps1`; `-NoInstall` keeps what's installed, e.g. a
  release's), writes `garrysmod\data\dms\bench.txt` (`dms_game_dir` from `golden\game_dir.txt`/`DMS_GAME_DIR`/`-GtaDir`,
  `dms_dev 1`, a new token) and starts **the game itself**:
  `gmod_win64.exe -w 1280 -h 720 -windowed -nosound -novid -usercon -condebug -ip 127.0.0.1 -port 27115 +maxplayers 2
  +sv_lan 1 +sv_cheats 1 +sv_hibernate_think 1 +rcon_password <new> +map gm_flatgrass`. The window is visible so the
  test can be watched; ticks run at 66/s unfocused and minimised too. `start -Hidden`: without drawing (`-textmode`,
  640×480), the window hidden (`ShowWindow`), as in M2.5.
  It's a listen server: `maxplayers 2`, otherwise single player without RCON; `-usercon` turns RCON on. The host is the
  user's Steam account on loopback. The addon reads `bench.txt` and deletes it at once, so a normal game doesn't open
  the channel. Addresses and secrets are in `.bench\server.txt` (not in git), `dms-ctl` reads them. The console log is
  `garrysmod\console.log`.
- `stop`: `quit` over RCON, forced after 10 s; `restart`, `status`.
- The game doesn't start twice: while the bench runs, the user doesn't play (and the other way round); `start` checks.
- `-Dedicated [-Server <dir>]`: the same on a dedicated server (`srcds_win64.exe`, SteamCMD app 4020, the x86-64
  branch; `update` installs or updates it). **The VPhysics-Box3D fork doesn't run there**: GMod's x64 server build is
  built against newer interfaces (`IAppSystem` with `GetDependencies/GetTier/Reconnect/IsSingleton`: 17 vtable slots in
  `VPhysics031` against 13 for the client and the fork; `VPhysicsCollision007` 59 against 52; `VPhysicsSurfaceProps001`
  longer too), and the fork crashes `dedicated.dll`/`server.dll`. So the server keeps its own `vphysics.dll`: the
  channel, the test agent and recordings work, `IPhysicsNM` doesn't. Porting the fork to the server's ABI is a task of
  its own (not done: ADR 0021).

## The module's channel (`module/src/dev.rs`, `addon/lua/dms/dev.lua`)

Only with `dms_dev 1` (0 by default), only on `127.0.0.1:dms_dev_port` (27099), only with `dms_dev_token` (the first
request is `auth`). The network threads touch neither Lua nor the physics (ADR 0003): they queue requests and hand out
finished frames. `Think` takes the queue (`DMS.DevThink`): console commands and Lua go back to Lua, NM messages and
scenarios to the test agent. A frame: a line `<id> <kind> <length>` and `<length>` bytes of UTF-8.

## `dms-ctl`

| Command | What | Path |
|---|---|---|
| `status` | what answers | RCON and the channel |
| `exec <command>` | a console command with its output | RCON (only it gives output); `--module`: into `Think` without output; if RCON is silent, the module |
| `lua <code>` / `lua -f <file>` | Lua in `Think`: what it printed and returned (an expression as `return …`) | the module; if it doesn't answer, RCON `lua_run` |
| `nm <message> [p=v …]` | an NM message to the test agent | the module |
| `bench reset\|stop\|cancel` | a new agent / remove it / break off the scenario | the module |
| `snapshot [--fields a,b]` | the last telemetry (JSON) | the module |
| `watch --for 2s [--fields a,b] [--every N]` | a stream of telemetry, a JSON line a server frame; **always with `--for`** | the module |
| `run <scenario> [--out f.rec] [--ref p.rec] [--lead]` | a `golden/scenarios/` scenario on Homer in FOLLOW (`--lead`: the LEAD agent) in real time → `.rec` (by default `golden/out/source/<name>.rec`); commands on the steps of the Playroom's recording (`--ref`, by default `golden/out/<name>.rec`) | the module |

PowerShell 5.1 drops `"` from external programs' arguments: in Lua use single quotes or `lua -f`.

Telemetry (top-level fields, `--fields` picks): `t` (`CurTime`), `tick`, `frame_dt`, `hover` (how many objects the
debug drives `DMS.Drive` hold), `gravity` (`nm` 9.8, `source` from `physenv`, `compensation`: the `dms_gravity_mode`
mode, ADR 0007), `source_gravity`, `nm_ms` (NM per physics step, ms), `agents` (FOLLOW agents: `id`, `step`, `awake`,
`asleep`, `balancing`, `fallen`, `pelvis_z`, `head_z`, `com`, `com_v`, `compensation`, `parts`, `contacts`, `drives`,
`armature`, `feedback`, `timestep`: the physics step, `pred_err`: how far Box3D went from the prediction of NM's own
physics over a step, `limit_use`: how full NM's joint limits are, `state_v`: velocities from `GetBodyState`, `script`),
`bench`; with the LEAD agent also `step`, `nm_t`, `mode`, `held`, `balancing`, `fallen`, `pelvis_z`, `head_z`, `com`,
`com_v`, `parts`, `contacts`, `drives`, `feedback`, `script`.

## Scenarios on Homer in FOLLOW (`module/src/follow.rs`, ROADMAP M4)

`dms-ctl run` (and `golden behaviour --source`) runs a scenario on **Homer in FOLLOW on Box3D**: Lua spawns Homer in
the Playroom's pose at the room's start (3 m higher for `!button fall`), the player where the Playroom's camera is (4 m
behind), and hands Homer to the module (`DMS.BenchFollow`); NM is an agent on the `delirium` crate with the outbox and
the Playroom's `in_air` forwarded (ADR 0016, 0017). A window scenario's commands go on the **NM steps** where the
Playroom ran them (by the `#` marks of its recording `golden/out/<name>.rec`, `--ref`; without a recording, by time from
the first command), `!poke` with the impulse from the Playroom's recording. The recording is the Playroom's `--rec`
format, in its tick's order; a part's `n`/`g` are NM's holding contacts (`held_world`) on Box3D's state. When it ends,
NM comes off Homer (until the next run he's an ordinary ragdoll).

It can do: NM messages, every behaviour `!button` of the Playroom (`Sim::lines`: `balance drunk windmill pedal writhe
rollup catchfall highfall stairs flinch brace look point lean relax stayup foetal grab overwall "STOP all"`, `shot`,
`shot back`; `fall` only at the start; `get-up` switches the get-up mode), the headless shot at step 60 (ShotTask), the
Playroom's reaction commands (M6, ADR 0020) `!shoot <part> [fatal]`, `!weapon <i>`, `!blast x,y,z`, `!miss <x>`,
`!set <key> <value>`, `!hfall [up]`, `!nododge`, and `!poke`, `!quit`. The reaction layer (shot, blast, death, getting
up by clip or muscles, the game's tasks, `in_air`) is the same code as Homer's in the game; a scenario's agent is
always on the Playroom's defaults and without tuning. It can't (and writes `# dms: not run on the bench: …`): `!grab`,
Hero, the Playroom's cars and other `!…`.

`golden behaviour --source [names]` compares the recordings with the Playroom **by the scenario's tolerances**: the
`#! tol` line in the scenario's file (`golden/README.md`): the COM gap, the final height, the fall, the settling time,
the commands' steps, the room layer's reactions. Within tolerance now: `stand` (60 s), `stand-1s`, `push`, `relax`,
`catchfall`, the first 60 steps of `shot`, the `stairs` stage; outside: `relax-1s`, `lowwall` (close), `kerb`'s final
pose, the landing in `rooftop` (ADR 0019). `--lead`: the former LEAD agent of the SDK on the Playroom's floor (as
`dms_selftest`): its recordings match the Playroom to three digits, except `push` (the camera), `catchfall` and `shot`.

`DMS.FollowStats()`: NM's step cost on the main thread, threads, the callback's calls (and from another environment),
each agent's step; `DMS.DevPanic("native"|"step")` checks that a panic doesn't crash the game (ADR 0021). After
`changelevel` the bench's convars (`dms_dev`, the token) are reset: turn the channel on again over RCON
(`dms_dev_token <from .bench\server.txt>`, `dms_dev 1`).

`server.ps1 start -Tickrate 60`: the server's tickrate 60; the physics step stays 0.015 s (the game sets it),
`DMS.EnvTimestep(physobj[, t])` reads and changes it (for experiments only).

## The scene and NM's world (M5, ADR 0018)

`gm_flatgrass`: a flat floor; the start of the Playroom's room is Box3D's floor (`DMS_FloorBelow`: the BSP plane + the
fork's 6.35 mm shell) under `info_player_start` (`dms_bench_scene`; `DMS.BenchScene()` checks the floor is level within
±5 m).

NM's world comes from the current map's `.bsp` (`world.lua`, `dms_map [reload]`, `[DMS] NM's world: …` in the log); the
floor is 1 m squares on the agent's frame, like the Playroom's floor. `DMS_MapCheck(pos, half, n)` compares the heights
of NM's world with the game's traces. Props within 3 m of the agent are NM's obstacles (`obstacles` in the telemetry),
`pivot_gap` is the gap of the joints' pivots on Box3D's state.

The Playroom's stages are the map `dms_stages` (made by `cargo run --release -p dms-map`: the `.vmf` in
`target/dms-map/`, the game's `vbsp/vvis/vrad`, `garrysmod/maps/dms_stages.bsp`; not in git): `bigstairs`,
`stairsdown`, `rooftop`, `highledge`, `lowwall`, `kerb`, `rail`, `open` and a slope, a site for each stage
(`info_target dms_stage_<name>`, `DMS.BenchScene(<name>)`). Stage scenarios (`#! window stage=<name>`) run on the bench
with `server.ps1 start -Map dms_stages`; the room's scenarios run on either map.
