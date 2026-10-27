# 0027. ValveBiped ragdolls: the swap, NM's own collision pairs (`VBOX3D_NM_006`), the fit in Homer's stance

Status: **accepted** (2026-10-28, ROADMAP M10). Refines 0025 (S2's input, S4's client side) and 0026 (the fork).

## Context

M9's prototype (`dms_spike_native`) made male_07's body natively and stood it under `bodyBalance`, but: it spawned in
the model's bind pose; its bodies didn't collide with each other at all; making one took 0.9 s; the game's ragdoll
couldn't be handed over to NM or back; Lua couldn't reach the native bodies (the physgun grabbed the model's frozen
objects); and the bones without a physics object were left 10–17 cm off. On the bench two more things showed up:

- **`BuildBonePositions` never runs for a server ragdoll on the client** (`C_ServerRagdoll` builds its bones without
  the callback): ADR 0025's client finishing can't go there.
- **The fitted body stood in ValveBiped's reference pose.** `Build::fit` fits Homer's build to the model's joints as
  they are; NM's standing frames are its neutral, so male_07 stood with its upper arms 40° out (Homer 15°) and its neck
  33–43° forward (Homer 15°).

## Decision

1. **The fork, `VBOX3D_NM_006`** (the strings `_005` … `_001` kept):
   - `SetBodyCollisionPairs(objects, n, pairs, npairs)`: the objects are one body for their own collisions; only NM's
     self-collision pairs (fred.xrt's, `dms_game::body::Body::pairs`) collide, decided before the game's filter is asked
     (`ShouldCollide`, `IsCollisionAllowed`, `EvaluateCollision`); contacts with anything else are the game's as before.
   - `SetBodyState(object, state)`: a teleport with velocities (SI), for handing motion over between the game's ragdoll
     and NM's body.
   - `CreateBody` keeps the hull made for a set of points and reuses it (0.9 s → 0.9 ms for a model's second body).
2. **The swap.** `dms_attach` / `dms.Attach` on any ValveBiped `prop_ragdoll`: NM's body takes the ragdoll's pose (the
   15 objects' bones through their bind offsets, the 6 parts without one between their neighbours) and every part's
   velocity; the model's objects are frozen, without collisions or gravity, its own constraints removed.
   `dms_detach` gives the model its physics back as a **new ragdoll** of the model in NM's pose and motion (skin,
   bodygroups, colour, materials, collision group, creator copied; `undo` and `cleanup` moved to it): GMod can't put
   a ragdoll's own constraints back.
3. **The fit in Homer's stance.** Before `Build::fit` the model's 21 joints are put in Homer's stance: each segment
   keeps the model's length and takes Homer's direction, turned to the model's facing; the model's bones turn with their
   parts. This is the mapping's input (as the spine's spread in 0025), not NM's tuning (0008 holds).
4. **The client's finishing** is done in the ragdoll's `RenderOverride`: `SetupBones`, the 6 bones set in their physics
   parent's frame as the server sends it (NW2), the bones under them without objects carried along, then drawn.
5. **The physgun and the gravity gun** on such a ragdoll don't pick up its objects: they grab NM's body (the
   Playroom's `pull`, `DMS.FollowGrab`); the gravity gun's punt is the Playroom's `poke`. **Bullets** hit the NM part of
   the physics bone they hit.
6. **Get-up by animation** stays Homer's (his clips): a fitted body with the animation's mode stays down (the user's
   choice); the muscles' mode runs.
7. **The golden of a fitted body** is its own build in LEAD on the bench (the crate as the Playroom runs it, the same
   scenarios and room layer: `Follower::lead`, `fit=<model> lead`), FOLLOW held to it by the scenario's tolerances
   (`golden fit`): the Playroom can't load a fitted build (0025).

## Consequences

- With the fork's `_004`/`_005` Homer works and GMod's ragdolls don't (a message says which version is needed).
- A detached ragdoll is a new entity; hooks get `DMS_Detached` on the old one, `dms.Detach` returns the new one.
- The leaves (feet, hands, head) turn with their parent in the stance: male_07's head is a little tilted back.
- The duplicator and saves of a ragdoll with NM on it copy a frozen ragdoll without its constraints (M11).
- `golden fit` on male_07 (the bench, 2026-10-28): `stand` and `catchfall` within the tolerances; `push` (COM mean
  0.061 m against 0.05) and `shot` (0.378 m against 0.25) off: the fitted body steps away further than its LEAD after a
  push or a hit (Homer 0.020 and 0.148 m), never falls, ends as high. Open: where Box3D parts from NM's physics on it.
