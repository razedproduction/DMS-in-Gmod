# DMS in Gmod

Delirium (an active ragdoll in the NaturalMotion style) natively in Garry's Mod, in **FOLLOW** mode: the body is an
ordinary `prop_ragdoll` on the [VPhysics-Box3D](vphysics/) physics (a fork with `IPhysicsNM` and joint armature), and
every physics step Delirium returns forces and torques for its parts. Homer balances, stumbles, braces and falls on
his own, and reacts to shots, blasts, fire and blows exactly as he does in the Delirium Playroom.

> The project is developed with [Claude Code](https://claude.com/claude-code).

## Playing

You need Garry's Mod's **x86-64 branch** on Windows and **your own copy of GTA IV** (PC or Xbox 360): Homer's body
and NM's data are read from it, nothing of the game is shipped. Download `DMS-<version>-win64.zip` from
[Releases](https://github.com/razedproduction/DMS-in-Gmod/releases) and follow [docs/install.md](docs/install.md):
installation, first steps (`dms_homer`, `dms_menu`), FAQ.

- [docs/install.md](docs/install.md): installation and FAQ.
- [docs/api.md](docs/api.md): the Lua API for mods, console commands, convars.

## Developing

- [ROADMAP.md](ROADMAP.md): the plan by milestones M0–M8 (Homer's body only for now).
- [docs/adr/](docs/adr/README.md): architecture decisions.
- [docs/bench.md](docs/bench.md): the bench in the game and `dms-ctl`; [golden/README.md](golden/README.md): the
  golden comparison against the Playroom; [docs/vphysics.md](docs/vphysics.md): the fork.

Building (Windows, Garry's Mod's x86-64 branch only; Delirium's repository next to this one, see
[docs/adr/0001](docs/adr/0001-rust-module-sdk-rlib.md)):

```powershell
.\scripts\build.ps1             # -> dist\gmsv_dms_win64.dll
.\scripts\build-vphysics.ps1    # -> vphysics\build\windows-x64\src\Release\vphysics.dll
.\scripts\install.ps1 -Fork     # the module, the addon and the fork into Garry's Mod (found through Steam, or -GarrysMod)
.\scripts\package.ps1           # everything built and packed: dist\DMS-<version>-win64.zip
```

## Licence

GPL-3.0-only, see [LICENSE](LICENSE). The exception is the VPhysics-Box3D fork in `vphysics/` (from
[Asphaltian/VPhysics-Box3D](https://github.com/Asphaltian/VPhysics-Box3D)): MIT, see [vphysics/LICENSE](vphysics/LICENSE).
Delirium, Box3D, Qhull, the Source SDK and the games: [THIRD_PARTY.md](THIRD_PARTY.md).
