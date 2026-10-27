# 0012. The bench: the game without a window and the `dms-ctl` channel in the module

Status: accepted (2026-10-21)

## Context

M2.5: Claude has to start GMod on its own, drive it and see its state (ROADMAP). The plan was a dedicated server
(`srcds_win64.exe`, SteamCMD app 4020, the x86-64 branch) with the VPhysics-Box3D fork. Checking showed that GMod's x64
server build is built against newer vphysics interfaces than the client and the fork:

- `IAppSystem` with four methods after `Shutdown` (`GetDependencies` → NULL, `GetTier` → 1, `Reconnect`, `IsSingleton` →
  true): the server's `VPhysics031` has 17 vtable slots, the client's and the fork's 13; `dedicated.dll` calls slot 5
  and crashes on the fork (in the fork that's `CreateEnvironment`);
- `VPhysicsCollision007` has 59 slots against 52, `VPhysicsSurfaceProps001` is longer too: with an adapter for
  `IAppSystem`, `server.dll` crashes as the map loads.

The game's client works with the fork (`IPhysicsNM` `VBOX3D_NM_002`, 4 sub-steps).

## Decision

1. The bench is **the user's game itself** (`gmod_win64.exe -textmode`, a listen server `maxplayers 2`, `-usercon` for
   RCON, everything on `127.0.0.1`), the window hidden; `scripts\server.ps1 start|stop|restart|status`. A dedicated
   server is the `-Dedicated` option with its own `vphysics.dll` (without `IPhysicsNM`); porting the fork to the
   server's ABI is a task of its own.
2. The bench's settings (`dms_dev 1`, the token, `dms_game_dir`) are in `garrysmod/data/dms/bench.txt`; the addon reads
   and deletes it: a normal game doesn't open the channel (GMod blocks `exec` from Lua, a `+convar` on the command line
   comes before Lua creates the convars).
3. The control and telemetry channel is **in the module**: TCP only on `127.0.0.1`, only with `dms_dev 1`, a token a
   start; the network threads only queue requests and hand out finished frames (ADR 0003), the work is done in `Think`.
   Console commands with output go through RCON (only it gives output), the rest through the module.
4. Until FOLLOW (M4) scenarios are run by an SDK agent in LEAD (as `dms_selftest`), writing a `.rec` in the Playroom's
   format; `golden behaviour --source` compares with `golden/out/`. In M4 Homer on Box3D takes its place.

## Consequences

- While the bench runs, the user doesn't play (one copy of the game); the listen server's host is their Steam account.
- The bench measures the client build: the same one the user plays.
- A dedicated server (and Linux, M7) needs the fork ported to the new `IAppSystem` and vphysics interfaces.
