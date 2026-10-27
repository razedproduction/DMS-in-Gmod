# 0004. A 1:1 reference body first, real models later

Status: accepted (2026-10-21)

## Context

NM's body is 21 parts (capsules along the local Y) and 20 joints (ball/hinge) with pivots, frames and limits. A
ValveBiped ragdoll has about 15 physics objects. `IPhysicsNM` is designed for "part = physics object, joint =
constraint" (`SetJointAnchor`, `SetJointLimits`, `SetJointArmature` on a constraint). Starting with ValveBiped, any
instability would mix integration errors with errors of fitting the body.

## Decision

- FOLLOW's first target is a reference model: 21 bones, 21 physics objects, the bone's frame the same as the NM
  capsule's (zero part offsets), masses, pivots and limits from NM's build.
- ValveBiped models (converted to 21 parts, or parts merged into fewer objects) come only after the reference stands
  and reacts reliably (milestone M4).

## Consequences

- A tool generating the model and its `.phy` (`tools/dms-ragdoll`, `studiomdl`) is needed before visible results on
  "real" models.
- The reference stays a regression test: if it breaks, the problem is in the integration, not the model.
