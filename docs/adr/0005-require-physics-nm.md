# 0005. Without `IPhysicsNM` the addon turns itself off

Status: accepted (2026-10-21)

## Context

FOLLOW needs continuous forces over all sub-steps and joint armature inside the solver: neither the standard VPhysics
(IVP) nor other replacements have it. Steam's file verification puts the original `vphysics.dll` back, so a missing fork
is an ordinary situation, not an exotic one.

## Decision

- On load the module asks the loaded `vphysics` for `CreateInterface("VBOX3D_NM_002")`.
- If it isn't there but `VBOX3D_NM_001` is, we work without `SetJointLimits` and print a warning.
- If neither is there, no agents are created, and the console and superadmin players get a clear message ("the wrong
  vphysics is installed, reinstall VPhysics-Box3D"). No fallback to the vanilla physics.

## Consequences

- The addon works only with the VPhysics-Box3D fork.
- The interface version has to be checked at start and at every map change.
