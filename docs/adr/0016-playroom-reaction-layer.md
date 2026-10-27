# 0016. The Playroom's reaction layer is carried over with the same code, through the `delirium` crate

Status: accepted (2026-10-26). Refines ADR 0008.

## Context

By Delirium's documentation (`docs/how-it-works.md`, `docs/modding.md`, v0.2.2) and `delirium-room`'s code:

- The SDK's C API (`nm_*`) is a thin layer over the `delirium` crate. Of the tasks it exposes only four:
  `nm_agent_task_shot`, `task_balance`, `task_catch_fall`, `task_flinch`.
- The engine has more tasks: `shottask`, `meleetask`, `explosion`, `onfire`, `playerctl`. The Playroom calls them
  straight from Rust.
- Behaviours send each other messages through `extras.outbox`: catchFall itself starts `pedalLegs` and `bodyRollUp`,
  and in v0.2.2 `stopAllBehaviours`. The Playroom forwards them to the agent after the step. The C API doesn't expose
  this outbox (noticed in M2.5).
- Much of what's seen in the Playroom isn't the engine's behaviours but the room layer in `delirium-room`. These are
  decisions of the kind "which messages to send on what":
  - health and damage (`delirium-data`'s `ped::damage`);
  - high fall when falling from a height;
  - near miss;
  - death;
  - getting up (`getup`);
  - additional reactions and their switches in `settings.txt`.
- The module already depends on the `delirium` crate directly (M3).

If GMod got a reaction table of its own, Homer would react "similarly", but not as in the Playroom.

## Decision

1. The Playroom's reaction layer is golden too. What the Playroom sends the agent, and when, on a shot, a blow, a
   blast, fire, a stun gun, a fall, a miss, death and getting up, the module repeats with the same code and in the same
   order.
2. Where the C API is enough, we use it. Where it isn't (the outbox, the engine's tasks, damage and health), the module
   calls the `delirium` and `delirium-data` crates as `delirium-room` does. That isn't interfering with Delirium (ADR
   0008): the calls are the reference's.
3. Our own code is only translating GMod's events (`EntityTakeDamage`, a fall, water, death) into this layer's inputs:
   the body part, the point, the direction, the weapon, the speed.
4. The Playroom's switches (`tasks`, `avoid`, additional and experimental reactions, `getup`) become convars with the
   Playroom's defaults. Golden runs go on the defaults (`--safe`).
5. The Playroom application's own features (Hero, Portal, Skate 3, graphics, GPU water) aren't carried over (ROADMAP
   "Deferred").

## Consequences

- In M4 the module forwards `extras.outbox` after every NM step. Only that way does catchFall match the Playroom.
- M6 is the port of `delirium-room`'s reaction layer, with golden scenarios for every reaction.
- Every Delirium update needs the reaction layer compared with the new `delirium-room`, as `tools/dms-game` in M2 (ADR
  0010).
