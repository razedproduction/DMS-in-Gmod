# Homer's model (ROADMAP M3)

`models/dms/homer.mdl` is a ragdoll whose physics is NM's Homer body one to one (ADR 0013). The model is made **on your
machine** from your copy of GTA IV and is never distributed (ADR 0006).

## Making it

```powershell
cargo build --release -p dms-homer
.\target\release\dms-homer.exe --game "D:\Games\Grand Theft Auto IV"
```

(From a release: `install.ps1 -Model -Gta "<GTA IV folder>"`, docs/install.md.) `--game` can be left out when
`DMS_GAME_DIR` is set, or `garrysmod\data\dms\game_dir.txt` or the repository's `golden\game_dir.txt` exists; Garry's
Mod is found through Steam (or `--gmod <GarrysMod folder>`). What it does:

1. reads Homer (`m_y_pmanhat_01`) with the Playroom's code (`tools/dms-game`): the build, the standing pose, NM's body;
2. writes `target\dms-homer\homer_ref.smd` (outside the repository: `%TEMP%\dms-homer`; the Playroom's look: NM's
   capsules and boxes, left parts red, right parts blue, the rest grey, translucent), `homer_phys.smd` (the hulls of
   NM's capsules and boxes), `homer.qc` (the Playroom's colour schemes as skins: 0 mixed, 1 all blue, 2 all red, 3
   solid blue, 4 mixed opaque), the materials `garrysmod\materials\models\dms\homer\*.vmt`; GTA IV's mesh and textures
   aren't used;
3. compiles with `studiomdl` from Garry's Mod's `bin\win64` → `garrysmod\models\dms\homer.*`;
4. rewrites the `.phy`: NM's masses, the centre of mass in the part's centre, NM's inertia diagonal, damping 0,
   constraints, NM's self-collision pairs; prints a check of the `.phy` against NM's body;
5. marks the model with `garrysmod\models\dms\homer_version.txt` (`model <n>`, `dms_game::MODEL_VERSION`): at start
   the addon compares it with `DMS.HomerModelVersion` and asks for a new model when the body has changed.

## In the game

- `dms_homer` (superadmin): Homer stands in the Playroom's pose in front of the player, facing them; his joints get
  NM's pivots, frames and limits and his parts NM's inertia tensors right away (`DMS.BindHomer`), and NM drives him in
  FOLLOW under `bodyBalance` (M4, ADR 0017): Homer stands.
- `dms_follow_send <message>`: an NM message line to every Homer under NM (`bodyRelax start=true`,
  `applyImpulse start=true partIndex=9 impulse=0,30,0`, `bodyBalance start=true`); `dms_status`: the agents, their
  flags, the gravity compensation and NM's cost; `dms_gravity_mode nm|hybrid|off`: the gravity compensation (default
  `nm`).
- Lua: `DMS_SpawnHomer(pos, yaw)`, `DMS_HomerParts(ent)`, `DMS_HomerReport(ent)`, `DMS_Follow(ent)`, `DMS_Unfollow(ent)`,
  `DMS_Send(ent, line)`; natives `DMS.HomerBody([pos, yaw])`, `DMS.Follow`, `DMS.FollowSend`, `DMS.FollowState`,
  `DMS.FollowEvents`; hooks `DMS_Feedback`, `DMS_Fallen`, `DMS_Detached`.
- A Homer spawned from the spawn menu is bound as it spawns while `dms_autoattach` is 1 (M6); a ragdoll of the model
  that isn't bound has its pivots in place (the bones start in NM's joints), but NM's limits, joint frames and inertia
  tensors come only with `DMS.BindHomer`.

## Checking

```powershell
.\scripts\server.ps1 start
.\target\release\golden.exe body          # --no-game: without the game
.\scripts\server.ps1 stop
```

`golden body`: the build against the Playroom's `NM_DUMP` and the SDK's C API (1e-5); the `.phy` against the build; in
the game, the frames of the 21 bodies (≤ 1 mm, ≤ 0.5°), masses, inertia (≤ 1 %), FindConstraint on the 20 joints, then
a 4 s fall without NM (everything finite, pivots together, joints within NM's limits ± 5°).

It needs the fork with `IPhysicsNM` `VBOX3D_NM_004` (ADR 0014, 0018: also the contact material of an NM part):
`DMS.BindHomer` gives every part NM's full inertia tensor (`SetBodyInertia`), the balls NM's semantics
(`SetJointBallLimits`), the hinges `SetJointLimits`.

## Visual check (for the user)

1. `dms_homer` in front of you: Homer stands in the Playroom's pose, facing you, and looks as in the Playroom:
   translucent capsules, left red, right blue, the middle grey (no purple-and-black checks, no inside-out faces); skins
   1–4 (`ent:SetSkin(n)`) are the Playroom's other schemes.
2. `vcollide_wireframe 1`: the 21 capsules and boxes sit inside the body (feet and hands are boxes), nothing sticks out.
3. A fall: Homer settles and lies still, doesn't shake or fly apart, knees and elbows bend only their own way.
4. The Playroom's face and skeleton (M6): the eyes blink and look where he's hit, the brows when he's scared; the
   skeleton over the body (`dms_eyes`, `dms_brows`, `dms_skeleton`), NM's overlay: `dms_overlay 1` (docs/api.md).
5. Drag him by an arm or the head with the physgun: the joints hold.

## Visual check of FOLLOW (M4)

1. `dms_homer` in front of you: Homer stands under `bodyBalance` for a minute and longer without falling, without
   visible jitter; his feet stay on the floor, don't sink or slide.
2. Nudge him in the chest with the physgun: he sways, may step, stays up, as from a poke in the Playroom.
3. `dms_follow_send bodyRelax start=true`: he goes limp and sinks to the floor (no explosion, no shaking on the floor).
4. Lift him 3–4 m with the physgun and let go: in the air he spreads his arms (High Fall), lands and lies.
5. `dms_status`: the agents, `balancing`/`fallen`, compensation 1.00 (`nm`), NM's cost ~1 ms per Homer.
6. 4–5 Homers side by side (`dms_homer` several times): they all stand, the game doesn't stutter.
