# DMS 0.1.0

The first release of DMS: Delirium, an active ragdoll in the NaturalMotion style, in Garry's Mod. Homer balances,
stumbles, braces and falls on his own, and reacts to shots, blasts, fire, blows, cars and falls the way the Delirium
Playroom's Homer does: the same engine, the same inputs, the same reaction layer.

**Needs:** Garry's Mod's x86-64 branch on Windows and your own copy of GTA IV (PC or Xbox 360). Nothing of GTA IV is
included: Homer's model and NM's data come from your copy, on your machine. No AVX2 needed.

**Install:** unpack `DMS-0.1.0-win64.zip`, close Garry's Mod, run
`powershell -ExecutionPolicy Bypass -File .\install.ps1 -Gta "<your GTA IV folder>"`, then `dms_homer` in a local game.
Details and FAQ: `install.md`.

## What's in it

- Homer in FOLLOW mode on VPhysics-Box3D: Delirium v0.2.2 drives a `prop_ragdoll` every physics step; Garry's Mod's
  gravity is left alone (the difference from Delirium's 9.8 m/s² is made up per part).
- The Playroom's reactions: shots by weapon (pistol to sniper, shotgun pellets), blasts, fire, the stun gun, melee blows,
  cars, props, near misses, things thrown at him, falls from a height, death, getting up by the game's clip or muscles.
- NM's world from the map (BSP faces, displacements, surface materials) and nearby props as obstacles.
- The DMS window (`dms_menu`): spawning, NM on and off, the Playroom's switches, reaction try-outs, tuning NM's
  behaviours with profiles (off by default: the Playroom's behaviour), task params, performance (agent limit, threads,
  LOD) and a Debug tab with versions and problems.
- The DMS toolgun tool: NM on/off, reactions where you hit, behaviours, a target to look at, point at, lean to, flinch
  from or grab, an inspector, a charged kick, throwing Homer, health, pins.
- Homer's face and skeleton as in the Playroom, a debug overlay.
- A Lua API for mods (`dms.Attach`, controllers, hooks, reactions): `docs/api.md` in the repository.
- Up to 8 Homers by default, NM stepped on several threads, far or still Homers parked.

## Known limitations

- Windows, Garry's Mod's x86-64 branch, single player or a listen server only: no dedicated server, no Linux, no 32-bit.
- Homer's body only: other ragdolls and NPCs' death ragdolls can't be put under NM yet.
- Not yet the Playroom's to the letter where Box3D's soft joints differ from Delirium's physics: a relaxing Homer settles
  at a different moment, the pose after tripping over a kerb or landing from a roof can differ, he may step after
  getting up where the Playroom's Homer stands still.
- Water (buoyancy) and ice aren't supported yet; the "learned" get-up (`dms_getup 3`) isn't ported.
- Steam's *Verify integrity of game files* puts Garry's Mod's own `vphysics.dll` back: run `install.ps1` again.
- Players joining your listen server need their own Homer model (it can't be shared) or they see an error model.

## Licences

GPL-3.0-only (DMS and Delirium); `vphysics.dll` is VPhysics-Box3D under MIT with Box3D (MIT), Qhull and Source SDK 2013
code (Valve's Source 1 SDK License): `THIRD_PARTY.md` and `licenses\`. Source: https://github.com/razedproduction/DMS-in-Gmod
(tag `v0.1.0`) and https://github.com/PVCPC/delirium-physics (tag `v0.2.2`).
