# 0025. GMod's ragdolls: an NM body fitted to the model, the model posed from it

Status: **proposed** (2026-10-27). To be accepted, with S1/S3/S4 decided, at the end of ROADMAP M9. Refines 0004 (real
models after the reference) and replaces the "Deferred: GMod's original models" note.

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

## Decision (proposed)

1. **The physics body of any ragdoll is an NM body fitted to its model** (`Build::fit` on the model's 21 mapped joints
   in its bind pose), not the model's own `.phy`. FOLLOW, the bind, the room layer and the golden comparison work on it
   as on Homer: it is an NM body, only its sizes differ. This is Delirium's own way (ADR 0008 holds: nothing of NM is
   tuned).
2. **The model keeps its look.** Its mapped bones are posed from the parts, each with its bind-pose offset to its part.
   Unmapped bones (fingers, toes, jiggle, accessories) follow their parents. Skin, bodygroups, colour and flexes stay.
3. **The physics body is made** either by the fork natively (a new `IPhysicsNM`, `VBOX3D_NM_005`: the 21 bodies and
   joints from the fitted build, with the model's `prop_ragdoll` the networked, traced and physgun-able entity) or as a
   generated proxy model (`dms-ragdoll`, `studiomdl`, cached). M9's spike S3 decides. The native variant is
   preferred: remote clients need nothing but the model, and no compile at first use.
4. **The skeleton** is read from the `.mdl` (mounted Workshop GMAs included) or through the game (S1 decides).
   ValveBiped is mapped automatically. Other humanoids are mapped by known rigs or a mapping editor, with mappings
   shared as text. Non-humanoids aren't supported.
5. **Nothing of a model is copied or shipped**: everything is computed at run time from what the game has mounted.

## Consequences

- `bind.rs`, `dms_game::body` and `golden body` generalise from Homer's body to any fitted body.
- The golden comparison of a model is the Playroom on its fitted build dump (`delirium-room <xrt> <build.txt>`).
  Whether Homer's derived links apply is S2's question.
- Bodies far from 1.8 m behave worse (Delirium's tuning). They get a warning, not a fix.
- The capsules don't match a model's mesh exactly; some clipping is accepted.
