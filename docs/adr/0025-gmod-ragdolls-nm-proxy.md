# 0025. GMod's ragdolls: an NM body fitted to the model, the model posed from it

Status: **accepted** (2026-10-28, ROADMAP M9: the spikes S1–S4 and the user's choice of their variants). Proposed
2026-10-27. Refines 0004 (real models after the reference) and replaces the "Deferred: GMod's original models" note.
The fork's part is ADR 0026.

## Context

0.1.0 drives only Homer, whose ragdoll is NM's body one to one (ADR 0013): 21 physics objects in NM's frames, NM's
masses, tensors, pivots and limits, the fork tuned against the Playroom (ADR 0014, 0017–0019). GMod's ragdolls are
different:
- ValveBiped has ~15 physics objects: one or two for NM's four spine parts, none for the clavicles and the neck;
- Workshop models have arbitrary skeletons, masses, hulls and joint limits, often poor;
- NPCs' death ragdolls are server `prop_ragdoll`s; players' are client-side.

Driving the model's own ragdoll in FOLLOW would mean merging NM parts into objects (summing drives, splitting states)
and living with its masses and limits. That isn't NM's body any more, the golden comparison loses its meaning, and every
model behaves differently.

Delirium's own SDK solves exactly this for other engines. The Unity bridge (`NmCharacter`, `fitToSkeleton`) maps the
humanoid's bones to NM's 21 joints: the four spine joints spread from Spine to Neck, a missing neck or clavicle goes
between its neighbours. `nm_build_fit` (`Build::fit`) then fits Homer's build to them: bone lengths, masses by the cube
of the scale. The host builds the 21 parts as its physics, NM drives them in FOLLOW, and the character's bones are
posed from the parts.

## Decision

1. **The physics body of any ragdoll is an NM body fitted to its model** (`Build::fit` on the model's 21 mapped joints
   in its bind pose, on Homer's build and standing frames), not the model's own `.phy`. FOLLOW, the bind, the room
   layer work on it as on Homer: it is an NM body, only its sizes differ. Nothing of NM is tuned (ADR 0008 holds).
2. **The skeleton is read from the `.mdl` in the module** (S1): `file.Read(model, "GAME")` (mounted Workshop GMAs
   included), `studiohdr_t` 44–49, the bind pose as the chain of the bones' default transforms. Not through the game
   (`util.GetModelMeshes`, `SetupBones`).
3. **ValveBiped's map** is `NmCharacter.Joints`' with one change (S2): NM's Spine1–Spine3 joints are spread between
   Spine and Neck **in Homer's proportions** (0.139, 0.290, 0.520 of the way), not evenly. This is the mapping's
   input, not NM's tuning.
4. **The physics body is made by the fork natively** (S3 (a), ADR 0026): `IPhysicsNM` `VBOX3D_NM_005` creates the 21
   bodies (NM's capsules and boxes as `dms-homer` hulls them, NM's masses) and their joints in the model's ragdoll's
   environment, the bodies standing in for the ragdoll's entity. The model's `prop_ragdoll` stays the networked,
   traced, damaged, undone and saved entity. No generated proxy model.
5. **The model is posed from the parts on the server, finished on the client** (S4): the model's physics objects
   (15 on ValveBiped) frozen, without collisions and without the ragdoll's own constraints, put every tick where their
   bones are on the parts (each bone's bind-pose offset to its part); the bones without an object that NM has a part
   for (Spine, Spine1, Spine4, Neck1, the clavicles) are put right on the client in `BuildBonePositions` from the
   networked objects; the rest (fingers, toes, jiggle, accessories) follow their parents. Skin, bodygroups, colour and
   flexes stay: it is the model's own entity.
6. **Other humanoids** are mapped by known rigs or a mapping editor (M12); non-humanoids aren't supported.
7. **Nothing of a model is copied or shipped**: everything is computed at run time from what the game has mounted.

## The spikes' numbers (M9, on the bench)

- **S1:** 97 of GMod's models (HL2's and GMod's player models) read, 96 with a complete ValveBiped (`zombie_classic`
  has no Neck1/Head1); the bind pose equal to the engine's (`util.GetModelMeshes`' bind matrices) to 0.000 units;
  3.5 ms a model against 147 ms. A Workshop GMA mounted with `game.MountGMA` (Terminator Nextbot's `arnold`, `t800nw`)
  reads the same way.
- **S2:** with the even spread male_07 weighs 130 kg (Spine0 ×1.78, mass ×5.6); with Homer's proportions male_07,
  combine, police, kleiner are 1.74 m and 81 kg, the women (female, alyx, chell) 1.66 m and 69 kg (Homer 1.77 m,
  77.7 kg). Every fitted body stands 10 s under `bodyBalance` in LEAD. **The Playroom can't run a fitted build**:
  `Build::undump` (its `<build.txt>`) lays the anchors out again from fred.xrt's asset (`asset_layout`) and the dump
  has no `size`/`heft`, so a fitted dump comes back as Homer (pelvis 0.920 m against Homer's 0.919). The golden of a
  fitted body is the crate run on the build in memory (as `golden lead` checks the crate against the Playroom bit for
  bit on Homer). Homer's derived links are already in his build, so the fitted build has them.
- **S3:** both prototypes held male_07 under `bodyBalance`. Box3D against NM's own physics over a step (`pred_err`):
  spawn 0–4 s 0.049 m/s mean (a), 0.052 (b), Homer 0.054; after a 60 N s push 0.45 (a), 0.40 (b), Homer 0.57. Joint
  pivot gaps 3–8 mm, as Homer's. Making the body: (a) 0.9 s (the hulls), (b) 7.3 s (`dms-homer --joints`, studiomdl
  and GTA IV's files), and a recompiled model that the game has loaded is only taken after a map change. Bullets: in
  (a) the trace and the damage come to the model with its exact physics bone (Head1, Spine2, Thigh, Calf); in (b) they
  hit the invisible proxy or the model, whichever is nearer. Hit groups are 0 on any ragdoll (Source's ragdolls trace
  their physics, not hitboxes). Not checked: a second client (one copy of the game); by design (a) sends clients the
  model only, (b) an entity whose model clients don't have. The physgun and gravity gun grab the model's frozen
  objects in (a): M10 forwards them to NM (the Playroom's grab).
- **S4:** posing the 15 objects costs 0.07 ms a tick; with the ragdoll's own constraints left on, Source's separation
  fix pulled the upper body back (head 22 cm off); without them the drawn physics bones are within 7 mm of the parts.
  The bones that only follow their parents are 10–17 cm off what NM's parts say (Neck1 15 cm standing): the client's
  finishing in 5.
- The fitted body spawned in the model's bind pose starts outside NM's limits (knee 2.6×, Spine3 2.4×, neck 2.2× the
  range) and is back within 0.5 s; M10 starts it in NM's standing pose or the ragdoll's current one.

## Consequences

- `bind.rs`, `follow.rs` and the room layer take any NM body (`BodyDef`): Homer's or a fitted one; `dms_game::body` and
  `golden body` generalise the same way.
- The golden comparison of a fitted body runs the crate on its build in memory, not the Playroom's `<build.txt>`.
- The native bodies don't collide with each other at all yet (the owner's `FVPHYSICS_NO_SELF_COLLISIONS`): NM's
  self-collision pairs need the fork's own filter (M10).
- Get-up by animation uses Homer's clips: on a fitted body it needs scaling or stays off (M10).
- Bodies far from 1.8 m behave worse (Delirium's tuning). They get a warning, not a fix.
- The capsules don't match a model's mesh exactly; some clipping is accepted.
