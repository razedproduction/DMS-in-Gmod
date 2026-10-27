# 0020. The Playroom's reaction layer in the module, translating GMod's events, getting up held on the clip, tuning by profiles

Status: accepted (2026-10-27). Refines 0009 (how tuning works), 0016 (what's carried over in M6 and how), 0017 (NM's
impulses past `doll.out`).

## Context

M6: Homer reacts to what the game does to him as the Playroom does (ADR 0016): a shot, a blow, a blast, fire, a stun
gun, a car, a miss, a fall, death, getting up. The Playroom's reactions are `Sim` methods in `delirium-room` (`fire`,
`shot_frame`, `blast_on`, `blast_frame`, `burn`, `zap`, `land`, `near_miss`, `car_react`, `obj_hit`, `throw`, `in_air`,
`self_preserve`, `extra_look`, `rise`, `musc_rise`, `button`, `lines`), not a crate: they can't be called, only
repeated. The game's tasks they start (ShotTask, explosion, OnFire, MeleeTask, bump, highfall, NMPose) are in the
`delirium` crate; damage is `delirium-data`'s (`ped::damage`, `weaponinfo.xml`).

## Decision

1. **A port of `Sim` method by method**: `module/src/follow/room.rs`: the same crate calls in the same order, the same
   constants (WHIZZ 1.5 m, BURN 6 s, SHOCK 2.5 s, BLAST_R 5 m, …), the same place in the NM step (`tick`: fire every
   step, ShotTask every other step, the blast, `in_air`, `self_preserve`; after the step, the end of a look or lean,
   `failed`; then `rise`). The room's state (health 200, death, tasks, timers, the generator `Rand::default()`) is each
   Homer's own; a single Homer runs on the same series of random numbers as the Playroom. The Playroom's visuals (blood,
   sounds, burns, dismemberment) aren't carried over; the blow's "ring" only for the face.
2. **Our own code is only translating GMod's events** (`addon/lua/dms/reactions.lua`, `DMS.FollowReact` lines): the
   damage type → a reaction (bullets → `fire` with the Playroom's weapon by GMod's weapon class; one tick's pellets → one
   `fire` with `more`, as `pellets`; a blast → `blast_on` at the blast's centre; fire, `IsOnFire` → `burn`; a blow →
   `land`; a stun gun → `zap`), a car touching within 8 steps → `car_react`, a prop → `obj_hit`, a bullet that missed
   (`EntityFireBullets` + a trace) → `near_miss`, thrown by the physgun or gravity gun → `throw`, Homer let go → `fall`
   (smartFall). The part is the doll's capsule nearest the point (`Ragdoll::nearest`, as in the Playroom), not the
   hitgroup.
3. **The push is the Playroom's, not GMod's.** A bullet, a blast and a blow push the body through the Playroom's
   `applyImpulse` (already carried to Box3D, ADR 0017), so GMod's own damage force on Homer is zeroed. A prop's or car's
   contact Box3D has applied itself: `obj_hit` here goes without its own `applyImpulse` (in the Playroom objects are
   kinematic).
4. **The Playroom's switches are convars** `dms_*` with the `--safe` defaults (tasks 0, look 1, avoid 0, getup 0,
   extra_reactions 0 / extra_pick 255, fire_reaction 1, dive_shot 0, dodge_gap 3, weak 0, god 0). A scenario's agent
   (the bench, golden) doesn't read them: it has the Playroom's defaults and the scenario's `!set`, and no tuning either.
5. **Getting up by animation = the bodies held on the clip.** The Playroom shows the game's clip (`get_up_fast` from
   the front or the back) while its doll lies hidden; at the end, a new agent on the clip's last pose (`Quat::IDENTITY`)
   and `bodyBalance`. In GMod the bodies can't be hidden: Lua freezes the 21 bodies (`EnableMotion(false)`) and sets
   them in the clip's pose every tick (`DMS.FollowPose`); NM meanwhile doesn't step or drive the body (a scenario's
   recording writes the lying doll, as the Playroom does); at the end the bodies are released, and the module makes the
   Playroom's agent on them. The clips come from the copy of the game with the same code
   (`tools/dms-game/src/getup.rs`, `Clips::start/stand/frames/end`). A difference: if getting up is interrupted (a
   shot), the Playroom goes on from the lying doll, the module from the pose where the clip stopped. "Muscles" is
   `task_pose` + `task_pose_control` and a push of the pelvis (`agent.push`), as `musc_rise`; "the learned network"
   (`learn.rs`, `getup.net`) isn't carried over: the module says what the Playroom says without `getup.net`, and Homer
   lies.
6. **Behaviours' impulses into NM's "internal" physics aren't carried over.** `shot` (`shot_out.impulse`: the hand to
   the wound and the wound to the hand, a pair equal and opposite) and `grab` put impulses in `Ragdoll::pending`, which
   is private; `step_follow` runs them only in the prediction. They can't be repeated without changing Delirium (the
   internal `nm_shot_step`, `nmgr` compute them); the pair's total impulse is zero, only the hand's internal jerk is
   lost. If it's needed, ask the SDK for access to `pending` (as to `doll.out`) rather than computing it ourselves.
7. **Tuning (0009)**: profiles `data/dms/profiles/*.json` (differences only); Lua gives the module the active one as
   text (`DMS.SetTuning`): the params' values are added to the lines that start a behaviour or change its params,
   unless the line sets them; taskparams are a copy of the game's file with values replaced (`TaskParams` of new
   tasks). The menu is built from `nm_message_*`: the API has no ranges for params, they're taken from the description's
   text or around the default.
8. **The face, skeleton and overlay are on the client.** Every 50 ms the server puts NM's view in NW2 (the velocity,
   death, the blow, the face's frame on the head capsule from the Playroom's `face`: `rage_link` and the standing pose's
   "forward"), the client draws by the head's bone: the face goes with the ragdoll without network delay. The skeleton
   by the bones (their origin is the joint's pivot, M3); capsules, drives, COM: `DMS.FollowOverlay` 15 times a second
   for superadmins with `dms_overlay 1`.

## Consequences

- Golden scenarios of reactions: `shot` (all 600 steps), `blast`, `nearmiss`, `shotgun`, `death`, `getup`, `tasks`: the
  Playroom's commands `!blast`, `!shoot`, `!weapon`, `!miss`, `!set`, `!hfall` run on the bench; the reactions' log
  lines (`said=`), the commands' steps, the fall and the height are compared.
- The result (the Playroom rebuilt from v0.2.2's sources: the former `target-playroom` was older and didn't know
  `!miss`, `!set`; the references recorded anew, `shot` and `catchfall` byte for byte as before): ShotTask sends the
  same messages on the same steps, the damage is the same (`shot`, `shotgun`, `death`, `tasks`: "game task" at 3.92 s
  against 3.85); a miss: a 4 cm COM gap; a blast: an 11 m flight within 0.29 m, the tumble after landing chaotic, like
  the `rooftop` landing; getting up: the same clip (1.50 s), but Box3D's body comes to rest sooner after `bodyRelax`
  (still 0.5–1.7 s earlier), and after getting up the stance from the clip's pose drifts and Homer steps (the Playroom
  stands): divergences of the stance's and settling's physics (ADR 0017–0019), not of the reaction layer. The earlier
  scenarios as in M5 (`relax-1s`, `kerb`, `lowwall` outside tolerance).
- Reactions without a Playroom scenario command (fire, a stun gun, Hero's blow, a car, a prop, a throw) are checked on
  the bench from the console (`dms_react`): what they send, by the log (`DMS_Reaction`, `dms_react_log`), against the
  Playroom's code.
- On a Delirium update, compare `room.rs` with the new `delirium-room` (as `tools/dms-game`, ADR 0010).
