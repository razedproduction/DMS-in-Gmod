# 0003. No Lua in the step callback, exchange through queues

Status: accepted (2026-10-21)

## Context

`IPhysicsNM::SetStepCallback` calls us from inside `IPhysicsEnvironment::Simulate`, before the step is solved. At that
moment it's only safe to read the bodies' states and set drives and armature; game calls get queued for the next step
or are unsafe. Lua code at that moment could create or remove entities and touch the physics.

## Decision

- No Lua function and no game function other than `IPhysicsNM`'s methods is called inside the step callback.
- Lua → module: the natives put commands (attach, detach, NM messages, reactions) in a queue; the queue is taken at
  the start of the next `OnStep`.
- Module → Lua: events (fell, NM's feedback, an agent taken off for a NaN) gather in a queue taken from the `Think`
  hook, which calls `hook.Run("DMS_*")`.

## Consequences

- Commands from Lua act with a delay of up to one physics step (≤ 1/66 s).
- The agent's state Lua sees (`GetState`) is a snapshot at the end of the last step.
