# 0028. Death ragdolls: NM on NPCs' and players' corpses, the killing hit as the room's death, a limit, hulls ahead

Status: **accepted** (2026-10-28, ROADMAP M11). Refines 0016 (the room layer's death), 0021 (the limit, copies) and
0027 (the swap).

## Context

M10 put NM on any ValveBiped `prop_ragdoll` (0027). Dying NPCs and players make ragdolls of their own, mostly not
that:

- NPCs make a **client** ragdoll unless `ai_serverragdolls` is 1; a client entity can't have FOLLOW (0002).
- Players' death ragdolls are **always client-side** (HL2MP's `hl2mp_ragdoll`, GMod's `ply:CreateRagdoll()`).
- The game pushes an NPC's ragdoll with the killing hit (`CalcDamageForceVector`), and for a zero force makes one up.
- On the bench: in `CreateEntityRagdoll` the ragdoll's objects have no velocity yet; a tick later they have it, but as
  the fork's *pending* velocity (`SetVelocity` keeps it for the next step), which `GetBodyState` doesn't report: NM's
  body took the ragdoll's pose but not its motion.
- A model's first NM body makes its 21 hulls in the fork's `CreateBody` (0.9 s; kept for the next ones, 0027): in the
  frame someone dies.
- The duplicator and saves copied a ragdoll with NM as a frozen one: its Lua fields (the agent, the native body) went
  into the copy, so the copy looked attached and nothing put NM on it.

## Decision

1. **NPCs.** With `dms_autoattach_npc` (on), a ValveBiped NPC's lethal damage (`EntityTakeDamage`, damage ≥ health)
   makes its ragdoll on the server (`SetShouldServerRagdoll(true)`) and takes the game's push off it (a 1e-4 N s force,
   so the engine doesn't make one up) for the hits the room pushes itself (bullets, blasts, blows). `CreateEntityRagdoll`
   gives the corpse the NPC's place in undo and cleanup, and a tick later NM goes on it (`DMS_AttachRagdoll`).
2. **Players.** With `dms_autoattach_players` (on), after the gamemode's `DoPlayerDeath` (`PlayerDeath`) a server
   ragdoll of the player's model is made, posed from the player's bones and moving as the player was, NM on it, and the
   gamemode's client ragdoll removed in the same tick (the clients never see it). Its look is the player's (skin,
   bodygroups, colour, materials, the player colour through the `PlayerColor` proxy), its collision group
   `COLLISION_GROUP_WEAPON` (the world and things, not players). The client's death camera follows it as HL2MP's (GMod's)
   follows its own: over the ragdoll's root (+14 units), 96 units back along the view, stopped by walls. One a player,
   removed at their next death or when they leave.
3. **The killing hit is the room's, on a fatally injured body** (0016): the corpse's room gets `health value=49`, then
   its last hit (≤ 0.5 s old) as the room's reactions translate the game's damage (reactions.lua: `shot`, `blast`,
   `melee`, `burn`, `taser`), its damage taken off that health by the room's own model (`ped::damage`). Under 50 the
   room's ShotTask plays its dying reaction (`shot` reaching for the wound, `shotRelax` 0.5 s, no `stayUpright`), and the
   room's get-up needs 50: it stays down; later hits react the same way till its health is 0, the room's death
   (`STOP all`, `relax`). No hit known (a fall, a crush, `kill`): `health value=0` at once. Not the Playroom's
   `!shoot <part> fatal` (golden `death`): that is health 0 at once, which cancels the shot's reaction on the next step,
   so a corpse only went limp (the user's choice, 2026-10-28: fatally injured).
4. **The motion handed over.** Before NM takes a ragdoll's pose, each moving object's velocity is made its own
   (`SetVelocityInstantaneous(GetVelocity())`), so `GetBodyState` sees what the game gave it this tick. Not a fork change:
   the fork's pending velocity is VPhysics' semantics.
5. **A limit.** `dms_autoattach_max` (4) death ragdolls have NM at once, within `dms_max_agents`; a new one takes NM off
   the oldest, as `dms_autoattach_time` (30 s) does; NM off is 0027's swap (an ordinary ragdoll in its place). M7's LOD
   parks the ones lying still. Removed by any means (a fade, cleanup, undo), NM goes with it (`OnRemove`).
6. **Hulls ahead.** An NPC's or a player's ValveBiped model is fitted and its hulls made as they spawn, a part a tick
   (`DMS.RagdollWarm`: a body made and destroyed at once, the fork keeps the hull).
7. **Copies.** A ValveBiped ragdoll with NM gets `PreEntityCopy`/`PostEntityCopy` as Homer has (0021): its fields stay
   out of the copy; the copy (marked `attached`, or a stale one from an older save) gets its own physics back, unfrozen,
   and NM again.
8. **`golden body --fit <model>`** checks a fitted body as `golden body` checks Homer's: the module gives the joints it
   fitted to (`DMS.FitJoints`), the tool fits Homer's build to them itself and holds it against the SDK's `nm_build_fit`
   on the Playroom's dump (1e-5), then on the bench the native body made standing (`DMS.RagdollReport`: frames, masses,
   damping, the inertia tensor, joints) against it, and a drop without NM. A fitted build's own dump can't be the
   reference: `Build::dump` doesn't carry the parts' sizes.

## Consequences

- With `ai_serverragdolls` 0 an NPC whose corpse got NM keeps a server ragdoll after NM is off (no client fading).
- Other addons that use `ply:GetRagdollEntity()` don't find a player's corpse while DMS makes it.
- A model's hulls cost ~0.9 s once a session, spread over ~21 ticks as it spawns (the worst tick 74 ms on the bench).
- On the bench (gm_flatgrass, 66 ticks): a citizen's death ragdoll in 1.1 ms with the hulls made ahead (914 ms without);
  8 NPCs shot 0.3 s apart with the limit 4: 4 corpses with NM, the server's tick 14.8 ms on average and 21 ms at worst
  (49 ms while the NPCs spawned), NM 3.2 ms a step for the 4 (M7: 1–2 ms an agent, NM's own). With the game's push left
  on, NM's body took the ragdoll's 44.6 m/s: the motion is handed over; with it off, 0.03 m/s and the room's impulse.
- `golden body --fit` on male_07, female_01 and combine_soldier: the build within 1e-6 of the SDK's, the native body's
  frames within 2.2 µm and 7e-7 rad, masses and tensors exact, a drop without NM within NM's limits.
