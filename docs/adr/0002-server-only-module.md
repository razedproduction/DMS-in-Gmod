# 0002. A server binary module only

Status: accepted (2026-10-21)

## Context

GMod's ragdolls are simulated on the server; the client gets their poses over the network. On a listen server two
physics environments live in one process: the server's and the client's.

## Decision

- Only `gmsv_dms_win64` is built (GMod's x86-64 branch; the 32-bit one isn't supported, Linux later). There's no
  client `gmcl_`.
- Delirium's agents go only on the server's environment (`IPhysicsNM::GetObjectEnvironment` for the parts of a
  server ragdoll).
- Client Lua is only debug drawing and UI.

## Consequences

- Players on someone else's server needn't install anything; only the host does (the module + the `vphysics` fork).
- Client ragdolls (`ClientsideRagdoll`, death ragdolls on the `cl_` side) don't get FOLLOW.
