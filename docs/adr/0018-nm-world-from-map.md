# 0018. NM's world from the map, obstacles, Box3D's shell and NM's contact materials (`VBOX3D_NM_004`)

Status: accepted (2026-10-27). Refines 0008 (the same NM inputs), 0016 (the room layer), 0017 (known divergences).

## Context

Before M5 the FOLLOW agent saw only the Playroom's flat floor at the spawn point's height (`data::floor_collision`):
over `gm_flatgrass`'s pit (a 13 m drop by the spawn point) NM went on "balancing" over the void for 2 s and didn't
notice the fall (no `in_air`, no HighFall). Besides:

| What | Delirium / Playroom | GMod before M5 | Divergence |
|---|---|---|---|
| NM's world | the room's floor in 1 m tiles + the stage's boxes (`world.add`) | a flat floor at the spawn's height | NM doesn't see pits, steps, walls |
| the floor's height | the floor = 0 in the room's frame | NM's floor is the BSP plane, Box3D's floor 6.35 mm higher (the fork's `SHAPE_INFLATION` shell) | NM's feet "hang" 6.35 mm above their floor; that masked the sinking in Box3D |
| props | `Obstacle` (cars, plates, planks) | none | NM doesn't see crates and barrels |
| contact friction and restitution | `μ_part × μ_surface` (foot 1.0, hand 2.25, the rest 0.75; the surface a materials.dat row), restitution `0.4 × e_surface`, unclamped | Box3D: `μ_flesh × μ_surfaceprop`, clamped to 1, restitution `sqrt(eA·eB)` | the feet slide differently, `kerb` falls in another place |
| ragdoll joints | rigid (an articulated body) | Box3D's soft correction at 60 Hz, ζ = 2, VPhysics-Box3D's "stiffness budget" | the supporting leg's ankle stretches 8–10 mm |

## Decision

1. **NM's world comes from the map's `.bsp`** (`module/src/world.rs`, `addon/lua/dms/world.lua`). Lua reads the map's
   file (`file.Read(..., "GAME")`), the module takes the world model's faces without `SKY/SKY2D/WARP/TRIGGER/NODRAW/
   HINT/SKIP` and the displacements (a `2^power + 1` grid, the `CDispVert` offsets), drops T-junction points and
   degenerate pieces, winds the faces anticlockwise about the outward normal (Source stores them the other way) and
   moves each face out by the fork's shell (6.3475 mm). The floor (normal `z ≥ 0.7`) is cut into 1 m squares on NM's
   frame's grid, like the Playroom's floor (`world::floor_tris`, the same vertex order and diagonal), within 50 m,
   farther in 64 m pieces. Each agent's NM frame starts on the floor under him (on the bench, the room's or stage's
   start). The cache: the parsed map by name for the session, NM's world by the frame's start (the last 4).
2. **Materials:** a face material's `$surfaceprop` (`Material(name):GetString`) → a materials.dat row by the table
   `world::SURFACES` (concrete → CONCRETE, brick → BRICK_WALL, grass → GRASS, …; unknown and `default` → CONCRETE, the
   Playroom's floor). It's translating GMod's input into NM's (0016, point 3); NM itself isn't touched.
3. **Fallback:** if the map can't be read (LZMA lumps, another format), the Playroom's floor at the height of a trace
   downwards (`DMS_FloorBelow`).
4. **Box3D's shell:** the floor under Homer (`DMS_FloorBelow`, `dms_homer`, the bench's scene) is the plane the trace
   hit (`HitPos` minus `DIST_EPSILON` 1/32), plus the shell. NM's world and Box3D's match to 0.1 mm (`DMS_MapCheck`).
5. **Props → `Obstacle`:** every tick Lua gathers the `MOVETYPE_VPHYSICS` bodies within 3 m of the agent (not ragdolls,
   not players or NPCs): the OBB (`OBBMins/Maxs`, the entity's axes), velocity, angular velocity, centre of mass, into
   `DMS.FollowObstacles`; the half-sizes plus Box3D's shell. NM gets them in `step_follow` until the next tick.
6. **The Playroom's room layer on the stages** (0016): `stairs_frame` (`rollDownStairs` when the balancer has given up
   and steps are ahead; the room's `ground` by NM's world) and the `rooftop` stage's `ledge` (armHang past the edge,
   then highFall), with the same code.
7. **The fork, `VBOX3D_NM_004`** (new methods at the end of the vtable, the strings `_003…_001` kept):
   - `SetBodyMaterial(obj, μ, e)`: an NM part's material (the module sets it in `DMS.BindHomer`:
     `dms_game::body::part_material` = Delirium's `part_friction`, `PART_ELASTICITY`);
   - `SetSurfaceMaterial(surfaceprop, μ, e)`: a materials.dat row's μ and e for a surfaceprop (Lua passes all the
     game's surfaceprops, the module maps them by the same table as NM's world);
   - Box3D's mixing callbacks: an NM part × an NM surface, or part × part, is `μA·μB` and `min(eA·eB, 1)`, as NM's
     physics; everything else is VPhysics's former mixing by surfaceprop (a tag in `userMaterialId`).
   - Joints between NM bodies (`IsNMDriven` on both) skip VPhysics-Box3D's "stiffness budget" and correct drift with
     Box3D's ceiling (¼ of the sub-step rate) and ζ = 0.2 (`NM_JOINT_*` in `physics_constraint.cpp`). The game's other
     joints don't change.
8. **The stages map** (`tools/dms-map` → `garrysmod/maps/dms_stages.bsp`, built with the game's `vbsp/vvis/vrad`, not
   in git): the Playroom's stages `bigstairs`, `stairsdown`, `rooftop`, `highledge`, `lowwall`, `kerb`, `rail`, `open`
   (their boxes are `dms_game::stage`, a copy of v0.2.2's `world::stage`) and a 20° slope, a site every 40 m along X,
   `info_target dms_stage_<name>`. `#! window stage=<name>` scenarios run in the Playroom with `--stage`, on the bench
   on this map (`server.ps1 start -Map dms_stages`); a new tolerance check `said=TEXT:S` (a room layer's log line).

## Consequences

- Over `gm_flatgrass`'s pit Homer falls as he should: NM sees the drop, `in_air` → HighFall, the landing at the bottom
  and catchFall.
- On a floor from the BSP NM gets the same world as the Playroom: `push` on the map's world and on the Playroom's floor
  match to a mm.
- The comparison (the bench, `golden behaviour --source`): `stand` 60 s, `stand-1s` (0.065 m, was 0.090), `push`
  (0.065/0.033 m), `relax`, `catchfall`, `shot`, the `kerb` stage (the fall 2.47/2.50 s) within tolerance; `rooftop`:
  the room layer's reactions within 0.05 s (the edge 3.15/3.20 s, lets go 6.17/6.22 s), but after highFall the
  Playroom lands on its feet (COM 0.92 m), our Homer lying.
- Known divergences (the plan is the next step in the fork's physics):
  - `relax-1s` (0.77 s against 1.00): the cause isn't the "soft contact" of 0017: the feet are no deeper than 4–7 mm in
    Box3D's floor (3.5–4 mm steady, like the contact's tolerance), but **the joints**: under the body's weight the
    supporting leg's ankle comes apart by 6–10 mm, the wrists and elbows up to 2 cm (the telemetry `pivot_gap`). It's
    Gauss–Seidel not converging on the "joint ↔ contact" chain at a light foot (77 kg on 1 kg): the fork's extra passes
    (0017) go over joints only, the feet's contacts aren't in them; the correction's stiffness (point 7) cut the
    stretch from ~9 to ~6 mm. The plan: solve NM bodies' contacts inside the extra passes (a scalar pass over NM
    bodies' contacts in `box3d-armature-iterations.patch`), then `relax-1s` again.
  - `lowwall`, `stairs`: the stance before the push is already 2 cm off (the same transient under loading), after
    hitting the wall or on the steps Homer takes other steps (goes along the wall; on the stairs the balancer gives up
    0.9 s earlier and the room layer sends `rollDownStairs`). A chaotic amplification of the same physics divergence;
    the same plan.
- The map's water (`buoyancy`) and ice aren't done (the next milestone for the world).
