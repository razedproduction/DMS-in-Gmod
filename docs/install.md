# Installing DMS

DMS puts Delirium, an active ragdoll in the NaturalMotion style, into Garry's Mod: Homer, a ragdoll whose muscles
keep him standing, so he balances, stumbles, braces and falls on his own and reacts to shots, blasts, fire and blows
as he does in the Delirium Playroom.

## What you need

- **Garry's Mod on the x86-64 branch** (Steam → Garry's Mod → Properties → Betas → `x86-64 - Chromium + 64-bit
  binaries`), on **Windows**. The 32-bit branch, Linux and dedicated servers aren't supported (see the FAQ).
- **Your own copy of GTA IV**: the PC version (the folder with `GTAIV.exe`) or Xbox 360 (the folder with
  `default.xex`). DMS reads Homer's body, poses and NM's character from it. No game files come with DMS and none are
  copied anywhere: everything is read from your copy on your machine.
- Any x86-64 CPU: **AVX2 isn't needed**.

## Install

1. Close Garry's Mod.
2. Unpack `DMS-<version>-win64.zip` anywhere.
3. In PowerShell, in the unpacked folder:

   ```powershell
   powershell -ExecutionPolicy Bypass -File .\install.ps1 -Gta "D:\Games\Grand Theft Auto IV"
   ```

   Add `-GarrysMod "D:\...\GarrysMod"` if Garry's Mod isn't found on its own.

What `install.ps1` does:

| What | Where |
|---|---|
| `bin\vphysics.dll` (VPhysics-Box3D, the physics DMS needs) | `GarrysMod\bin\win64\vphysics.dll`; Steam's own is kept once as `vphysics.dll.original` |
| `bin\gmsv_dms_win64.dll` (the DMS module) | `GarrysMod\garrysmod\lua\bin\` |
| `addon\` (the Lua addon) | `GarrysMod\garrysmod\addons\dms\` |
| `-Gta <folder>` | the folder's path into `garrysmod\data\dms\game_dir.txt`, then `bin\dms-homer.exe` makes Homer's model |

`dms-homer` builds `garrysmod\models\dms\homer.mdl` (and its materials) from your GTA IV with Garry's Mod's own
`studiomdl`: Homer's physics is NM's body exactly, drawn as the Playroom draws it (coloured capsules and boxes). The
model is yours: it isn't shared and can't be. To make it again (after a DMS update that asks for it):

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 -Model -Gta "D:\Games\Grand Theft Auto IV"
```

### By hand

The same steps without the script: copy the three files and the folder as in the table (keep a copy of the original
`vphysics.dll`), then run `bin\dms-homer.exe --game "<GTA IV folder>"` and, in Garry's Mod,
`dms_game_dir "<GTA IV folder>"`.

## First steps

Start a local game (single player or a listen server you host) on any map.

- `dms_homer` in the console (`~`): Homer stands in front of you, balancing.
- `dms_menu` opens the **DMS window** (bind a key in its Homer tab, or hold `C` and click the DMS icon): spawning,
  the Playroom's reactions and switches, behaviour tuning, performance, and a Debug tab with versions and checks.
- Toolgun → Tools → **DMS**: put NM on or off a Homer, reactions where you hit him, his behaviours, a target to look at
  or point at, an inspector, a kick, throwing Homer, health, pins.
- Shoot him, hit him, blow him up, set him on fire, drop him from a roof, throw things at him: he reacts as the
  Playroom's Homer does.
- `dms_selftest` checks that NM's data reads from your GTA IV; `dms_info` shows the versions and anything wrong.

The server's settings (console commands, the window's server tabs, the tool) are for superadmins: in single player
and on a listen server you host, that's you.

## FAQ

**The console says "vphysics.dll isn't VPhysics-Box3D" / DMS is disabled.**
Steam's *Verify integrity of game files* (and some Garry's Mod updates) put the original `vphysics.dll` back. Run
`install.ps1` again (without `-Gta` it only reinstalls the files).

**Homer doesn't appear / `dms_homer` says the model isn't there.**
Homer's model is made on your machine: run `install.ps1 -Model -Gta "<GTA IV folder>"`. If the console says
"NM's data isn't loaded" or "No GTA IV folder", DMS doesn't know where your GTA IV is: `install.ps1 -Gta …` or, in the
console, `dms_game_dir "<GTA IV folder>"` and restart Garry's Mod (the data loads once a game). The DMS window's Debug
tab lists what's missing.

**"gmsv_dms_win64.dll is X but the Lua addon is Y".**
The module and the addon come from different releases (often the Workshop's addon is newer or older than the module):
install both from one release.

**"homer.mdl was made by an older dms-homer".**
The new release changed Homer's body: make the model again with `install.ps1 -Model -Gta …`.

**Does it work on a dedicated server or Linux?**
No. Garry's Mod's x86-64 dedicated server is built against newer physics interfaces than the game, and VPhysics-Box3D
doesn't run there; there's no Linux build either. DMS runs in the game: single player or a listen server.

**Does the 32-bit branch work?**
No: only the x86-64 branch.

**Do other players on my listen server need DMS?**
The physics and the module are the host's only, and the addon's client part (Homer's face, the window, the tool)
comes to them from the server as Garry's Mod sends any addon's Lua. Homer's model doesn't: it's made from GTA IV and
isn't shared, so a player without their own (`install.ps1 -Model -Gta …`) sees an error model.

**Does DMS change my other games or Garry's Mod's physics?**
VPhysics-Box3D replaces Garry's Mod's physics engine for everything (it's a Box3D port of VPhysics), not only for
Homer; `sv_gravity` and the rest of the game aren't touched. To go back: `install.ps1 -Uninstall`.

**Can I put my own ragdolls under NM?**
Not yet: DMS works with Homer's body only (NM's body exactly). Other models are planned as a separate milestone.

## Uninstall

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1 -Uninstall
```

Puts Steam's `vphysics.dll` back and removes the module and the addon. Your Homer model, its materials and
`garrysmod\data\dms` (profiles, the GTA IV path) stay; delete `garrysmod\models\dms`, `garrysmod\materials\models\dms`
and `garrysmod\data\dms` by hand if you like.

## For modders

The Lua API (`dms.Attach`, controllers, hooks, reactions): `docs/api.md` in the repository
(https://github.com/razedproduction/DMS-in-Gmod). Building from source: the repository's README.
