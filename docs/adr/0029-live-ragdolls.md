# 0029. Live bodies: a shot NPC and a drunk player become their NM ragdoll and come back on their feet

Status: **accepted** (2026-10-28, ROADMAP M11.5). Refines 0016 (the room layer on a live body), 0021 (the limit, a Lua
restart) and 0028 (dying as a ragdoll).

## Context

M11 put NM on the ragdolls of the dead (0028). The living stayed the game's: an NPC shot and not killed flinches by
its animation, and a player can't be a ragdoll at all. The same three steps would give both:

- **into the ragdoll**: a server ragdoll of the model, posed from the entity's bones and moving as it was (M11's player
  corpse, `DMS_RagdollFrom`), NM on it (`DMS_AttachRagdoll`);
- **the room's reaction** on it (0016): a shot's `shot at= dir= weapon= part=` (no `fatal`), a button;
- **back**: the entity where the body is, the ragdoll gone. A fitted body doesn't get up by animation (0027, the
  user's choice), so an NPC is put back in the ragdoll's place, not played up by a get-up clip.

What the game has to keep meanwhile: the NPC's health, relationships, squad, weapon, AI; who kills it and the kill's
hooks. On the bench (gm_flatgrass, 66 ticks, citizens and the player's model):

- `GetBoneMatrix` on the server is current for an NPC in the tick a bullet hits it (the hit's trace set the bones up)
  and for a player (M11).
- Hits on the ragdoll have no hitgroup (its traces are vphysics'), so GMod's own scaling (`ScaleNPCDamage`:
  the head ×2, limbs ×0.25) was missing: a leg hit took 30 on the ragdoll and 7 on the NPC.
- `DMS.FollowRoom` didn't say whether the room's ShotTask was still running, which is when the room is done with a shot.
- Under `drunk` (the Playroom's button: `bodyBalance armsOutOnPushTimeout=999`, `leanInDirection 0.2` for 1.5 s) Homer
  doesn't fall in the Playroom (the new golden `drunk`), nor on the bench, nor male_07's fitted body in LEAD or FOLLOW
  from NM's stance (`golden fit drunk`). A drunk player's body, posed from their bones, fell in 2 runs of 3 at ~2 s; from
  NM's stance at their feet, in 2 of 2; a citizen spawned standing (`dms_ragdoll`), in none of 3; Homer spawned in the
  game, in 1 of 1. The drunk wander is chaotic: the bench's Homer and the Playroom's part by 1.86 m (COM, max) and end
  as high (0.934 against 0.943 m).

## Decision

1. **Live NPCs** (`dms_live_npc`, on): a ValveBiped NPC's bullet damage below its health (`EntityTakeDamage`), with
   fewer than `dms_live_max` (4) such NPCs and room in `dms_max_agents`, makes its ragdoll (posed from its bones, its
   velocity), NM on it, and the bullet goes to it as the room's shot (reactions.lua's queue: one shot a tick, a shotgun's
   other pellets `more=`); the game's push is taken off (the room's `applyImpulse` is it). The game's damage itself goes
   to the NPC as before (its health).
2. **The owner hidden**, not removed: `SetNoDraw`, not solid, `MOVETYPE_NONE`, its weapon not drawn; an NPC with
   `FL_NOTARGET` and `EFL_NO_THINK_FUNCTION` (its AI stops, its schedule and memory kept). Ten times a second it's moved to
   the floor under the body's pelvis (the server sends what's near it; a dying NPC drops its weapon there). Damage on the
   hidden owner is blocked, except a bullet of the tick that made the ragdoll (a shotgun's other pellets: the ragdoll's
   reaction too).
3. **Hits on the ragdoll are the owner's**: each is passed on (`TakeDamageInfo` with the same attacker, inflictor, type,
   point and force), a bullet's scaled by the gamemode for the hitgroup it hit on the owner (`ScaleNPCDamage`,
   `ScalePlayerDamage`: the hitbox on the owner's bone that the physics bone hit is). Their reactions are the room's
   (reactions.lua, as on any ragdoll with NM).
4. **Back**: an NPC once its body has settled, after at least 1 s — the room's ShotTask and explosion task over
   (`DMS.FollowRoom`: `shot`, `blast`, new; read-only, NM untouched) and the COM under 0.3 m/s for 0.5 s balancing or
   1.5 s lying (or parked by LOD) — or after `dms_live_time` (15 s); NM coming off the ragdoll (NaN, a limit) or the
   ragdoll removed brings it back at once. It stands on the floor under the pelvis (raised a little till its hull is
   free), facing as the body faced (its thighs, as NM's face takes it); health, relationships, weapon and AI are as they
   were. The ragdoll is removed.
5. **Dying as a ragdoll** is M11's death on that ragdoll (`DMS_DiedAsRagdoll`): forwarded damage kills the owner as
   the game does (credit, the kill feed, `OnNPCKilled`); the game's own death ragdoll (made on the server for it,
   `SetShouldServerRagdoll`) is removed in `CreateEntityRagdoll`, a player's client one in `PlayerDeath`; the live ragdoll
   gets `health value=49` (a known hit ≤ 0.5 s old; else 0) and joins the death ragdolls' limit. The killing hit's
   reaction is already on it (point 3), so it isn't sent twice. Undo and cleanup take an NPC's corpse in its place. With
   death ragdolls off for it, an NPC's becomes an ordinary ragdoll (the swap), a player's goes (GMod's own stays).
6. **Drunk** (`dms_drunk`, the DMS window's button): the player's ragdoll as above, the Playroom's `button name=drunk` on
   it. The player's own moves and buttons are cleared (`StartCommand`, both realms), weapon switching blocked, the view
   model and crosshair hidden; the camera is M11's death camera on the ragdoll (cl_ragdolls.lua). `dms_drunk` again: they
   stand where the body is, their view kept. Dying drunk, the ragdoll is their corpse; a spawn while drunk takes the
   ragdoll away. The start is the player's pose (as their corpse), not NM's stance: falling or not is NM's drunk on that
   pose (0008: not tuned).
7. **A Lua restart** keeps the live bodies (`DMS_Lives`, as `DMS_Agents`).
8. **Golden `drunk`** (a window scenario: `get-up` ×2, `drunk` at 3 s): Homer's references for the button.

## Consequences

- An NPC shot while a ragdoll isn't animated: it lies or staggers under NM till it settles, then pops up standing in
  its place (no get-up clip for a fitted body, 0027).
- Each new live body costs NM's world crop (`DMS.RagdollFollow`, ~28 ms on gm_flatgrass: the module keeps worlds by the
  exact origin of NM's frame, which is each body's own), as an M11 death ragdoll does; its native body ~1 ms, the
  ragdoll ~1 ms. On the bench 6 citizens shot 0.3 s apart with `dms_live_max` 4: 4 ragdolls, 2 hit as the game does; the
  server's tick 15.0 ms on average (66 ticks: 15.15), 63.7 ms at worst; NM 2.5 ms a step for the 4.
- The drunk body often falls within ~2 s from the player's pose (above), then lies (a fitted body doesn't get up by
  animation): `dms_drunk` again stands the player up.
- Other addons see the hidden NPC or player as it is (not drawn, not solid); NPCs don't target a hidden NPC.
- The duplicator and saves of a live ragdoll copy a ragdoll with NM, not the NPC behind it.
- Golden `drunk` on Homer: no fall in either, the end height within 0.02 m and the commands' steps as the Playroom's;
  the COM within 0.10/0.05 m it isn't (1.86 / 1.21 m: the wander parts at step 4 by 1.5 mm and grows).
