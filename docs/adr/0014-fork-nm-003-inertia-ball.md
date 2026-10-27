# 0014. The fork's `VBOX3D_NM_003`: the full inertia tensor and a ball with NM's semantics

Status: accepted (2026-10-22). Refines ADR 0013 (removes its two divergences from the fork).

## Context

`golden body` (M3) showed two divergences of Box3D from NM's physics that neither the model nor NM can fix (ADR 0008):

1. **Inertia.** The fork keeps a body's inertia diagonal in its frame (`m_Inertia`, `b3MassData` on the diagonal). NM's
   part tensor isn't diagonal in the part's frame (for the arms the off-diagonal terms are up to 46 % by norm).
2. **The ball.** `SetJointLimits` builds a twist/cone/plane ragdoll constraint: the twist axis picked by a heuristic
   (`SelectTwistAxis`), the cone circular, and `limits[i]` taken as the limits about axis `i`. In NM (`ragdoll.rs`
   `lean_limit`, `rigid::leans`) the twist is about the joint frame's Z, and the lean (turning the parent's Z into the
   child's Z about an axis `s` in the parent's XY plane by an angle `t`) is limited by the ellipse
   `(t·s.y/l0)² + (t·s.x/l1)² ≤ 1`: `limits[0]` holds the turn about the parent's **Y**, `limits[1]` about X. The left
   hip went ~15° past NM's limit in a fall.

## Decision

A new version of the fork's interface, `VBOX3D_NM_003` (the strings `_002` and `_001` give the same object, their
vtables are a prefix):

- `SetBodyInertia(obj, tensor[9])`: a 3×3 tensor (kg·m², about the centre of mass, in the body's frame) goes into
  `b3MassData` whole; `GetInertia` and VPhysics's other diagonal approximations take its diagonal; `SetInertia` resets it,
  `SetMass` scales it. `GetBodyInertia(obj, tensor[9])`: the tensor Box3D actually turns the body with (a check).
- `SetJointBallLimits(c, parent, frames[2], limits[3])`: NM's ball on a ragdoll constraint: Box3D's spherical joint with
  NM's frames, twist ±`limits[2]`, an elliptical cone in the parent's frame. The ellipse is the Box3D patch
  `cmake/patches/box3d-elliptical-cone.patch` (`b3SphericalJointDef::coneAngleY`: the limit along the lean axis `s` in
  body B's frame is `1/√((s.x/coneAngle)² + (s.y/coneAngleY)²)`, computed while preparing the step). The fork's ragdoll
  solver's cone adjustments and "plane" rows are off for such a joint. NM's hinges still go through `SetJointLimits`.
- The module requires `_003` for `DMS.BindHomer` (on `_002`, a clear error); `bind.rs` sets every part's tensor and NM's
  balls and hinges. The header's copy is `module/src/ffi/vphysics_nm.h`.

## Consequences

- `golden body` passes whole: Box3D's tensor = NM's (≤ 7e-8), joints in a fall ≤ 0.5° past NM's limits (the softness of
  Box3D's limits), pivots ≤ 0.6 mm.
- NM measures twist in a swing-twist decomposition relative to the parent, Box3D relative to the child; with NM's
  symmetric limits it's the same thing.
- The ellipse is taken along the lean axis at the step's start (as is Box3D's circular cone): the axis isn't refined
  within the step.
- Limitations: `SetJointBallLimits` needs the parent to be the constraint's attached object (that's how the game builds
  ragdolls); save/restore of such a joint doesn't keep its shape (as with the fork's other NM changes).
- The fork's NM extension (with `_003`) still isn't committed in the fork's repository (ROADMAP M0).
