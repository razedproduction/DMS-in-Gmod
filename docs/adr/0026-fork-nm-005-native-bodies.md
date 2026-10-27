# 0026. The fork makes NM's bodies: `IPhysicsNM` `VBOX3D_NM_005`

Status: **accepted** (2026-10-28, ROADMAP M9). Refines 0014, 0018 (the fork's interface); serves 0025.

## Context

ADR 0025 drives any model on an NM body fitted to it. Homer's body is a compiled ragdoll (`dms-homer`, ADR 0013), but a
model's fitted body can't be: compiling a proxy per model takes seconds, needs studiomdl and GTA IV's files, isn't
taken by a running game once it has loaded that model, and gives clients an entity whose model they don't have
(M9's S3 (b)). Lua can't make physics objects without an entity. So the fork has to.

## Decision

`IPhysicsNM` gets a fifth version, `VBOX3D_NM_005`, the older strings kept (`VBOX3D_NM_004` … `_001` give the same
object, their vtables shorter):

- `CreateBody(environment, owner, points, count, mass, position, rotation, gameFlags)`: a moving convex hull of the
  points (m, the body's frame, its centre of mass on the origin; NM's capsules and boxes as `dms-homer` hulls them, so
  the fork's 6.35 mm shell grows them to NM's size), NM's mass, in the environment of the model's ragdoll. The body
  takes the owner object's game data, material and callback flags, and its game flags with `FVPHYSICS_NO_SELF_COLLISIONS`
  added: the game hears of its contacts as the model's entity's and doesn't let it touch the entity's other objects.
  The fork keeps the hull until `DestroyBody`.
- `CreateJoint(parent, child, hinge)`: a ragdoll constraint as the game's ragdolls have it (the child the reference),
  a hinge with one free axis as VPhysics builds ragdoll hinges; wide open until the bind (`SetJointAnchor`,
  `SetJointLimits`, `SetJointBallLimits`, as for Homer) gives it NM's pivot, frames and limits.
- `DestroyJoint`, `DestroyBody`.

Everything else (the bind, FOLLOW, the drives, the armature, NM's materials) is the interface as it was: the bodies are
`IPhysicsObject`s like Homer's.

## Consequences

- The module asks for `VBOX3D_NM_005` first; with `_004` Homer works and GMod's ragdolls don't (a message says so).
- Making a body takes 0.9 s (the hulls, built each time); M10 caches the hulls per fitted build.
- The bodies don't collide with each other at all; NM's own self-collision pairs (fred.xrt) need a pair filter in the
  fork (M10, a later version).
- The bodies have no entity of their own: game code that walks an entity's objects (the physgun, the gravity gun,
  tools) sees the model's ragdoll; forwarding its grabs to NM is the module's work (M10).
