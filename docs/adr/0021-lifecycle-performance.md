# 0021. Agents' lifecycle, the parallel step, LOD, cropping the world, panics at the FFI boundary

Status: accepted (2026-10-27). Refines 0003, 0012, 0018.

## Context

M7: agents have to survive everything the game does to a ragdoll, not crash the game and fit in a budget. Measurements
on the bench (the user's machine, 8 logical cores, no AVX2), `gm_flatgrass`, Homer balancing:

- NM's own step (`Agent::step_follow`) is **1.2–2.1 ms** an agent (lying, 0.06–1.6 ms); everything of ours around it
  (reading bodies, the room layer, drives) ~0.02 ms. The Playroom on its own physics spends ~1.5 ms on Homer's step: it's
  the price of Delirium, which can't be changed (ADR 0008). The ROADMAP's target "≤ 0.15 ms a step an agent" is reachable
  only for our layer;
- 8 agents: ~14 ms a physics step on the main thread;
- every spawn built NM's world from the whole map under its frame's start: a ~175 ms freeze and ~50 MB a world;
- in FOLLOW a lying Homer hardly ever falls asleep (drives keep the bodies moving, the COM's speed "trembles" at
  ~0.1 m/s without moving), so "sleeping" by NM's flag hardly happens;
- the duplicator and GMod's saves (`gm_save` = `duplicator.CopyEnts`, loading = `game.CleanUpMap` +
  `duplicator.Paste`) merge the entity's Lua table into the copy: a Homer copy got someone else's `DMS_Agent`,
  `DMS_Rising`, callbacks;
- `catch_unwind` in one frame with `lua_error` crashed `lua_shared` on load (LuaJIT's SEH unwinding through a frame with
  Rust's catch funclet).

## Decision

1. **The step in three phases** (`step.rs`): `Follower::begin` on the main thread reads Box3D's bodies (`GetBodyState`)
   and checks the ragdoll's joints (`FindConstraint` = the remembered one, else NM comes off); `Follower::compute`, NM's
   steps without `IPhysicsNM` or the game, agents in parallel on `dms_threads` threads (`thread::scope`, 0: cores less
   one); `Follower::finish` on the main thread writes armature and drives. Box3D doesn't move between the NM steps of
   one Source step, so one read for all of them is the same as before; an agent's result doesn't depend on the thread
   (Delirium's global state is only `OnceLock`/`RwLock`/atomics). A single agent (golden) is computed on the main
   thread. 8 agents: 14 → 4.3 ms on the main thread.
2. **Panics**: a native's body is under `catch_unwind` in a separate `#[inline(never)]` function, `lua_error` in another,
   with no live values; a panic becomes a Lua error, a Lua error from a metamethod passes straight through. An agent's
   step is under `catch_unwind`, a panic takes only it off (`detached`, `panic: …`); the whole callback too, all of them
   taken off as a last resort. `gmod13_open/close` are under `catch_unwind`, `step::shutdown` takes the callback off
   first. The check: `DMS.DevPanic("native"|"step")` (only with the `dms-ctl` channel).
3. **The lifecycle** (`lifecycle.lua`, `follow.lua`): removal / `undo` / cleanup through `OnRemove`; a map change through
   `ShutDown` and `gmod13_close`; a Lua restart: `require` once a Lua state, agents live on; copies: `PreEntityCopy`
   takes the `DMS_*` fields out of the copy, leaving `DMS_Copy`, the copy (and a copy from an old save with someone
   else's agent) is bound in any pose (binding doesn't depend on the pose) and gets NM if the original had it. Every way
   of taking an agent off goes through one `forget`: a Homer on the get-up clip is unfrozen. A part frozen by a player:
   parked until unfrozen; picked up with the physgun or gravity gun on the clip: the clip is broken off
   (`DMS.FollowRiseCancel`), a new agent on the body balances, as at the clip's end. Welds and ropes are outside forces,
   NM stays.
4. **Teleporting**: the pelvis farther than 1 m (+ 2·v·dt) from where its velocity was taking it: NM's frame (and the
   world) move by the jump; the agent, its behaviours and the room are kept, NM doesn't see the jump. Not for a
   scenario's agent. `Entity:SetPos` on a `prop_ragdoll` doesn't move the bodies: `PhysObj:SetPos` does.
5. **LOD is parking**, not detaching: NM off the bodies (armature 0, no steps or drives: an ordinary ragdoll), the agent
   and the room (health, death) kept. Farther than `dms_lod_distance` from every player: the `relax` button, after 1 s
   parking, back when a player is near. Still (not balancing, not on the clip, the COM moving < 2 cm/s) for
   `dms_lod_still` s: parking, back when the module sees the bodies move (after 1 s of settling: speed > 0.5 m/s or
   2 rad/s), or before a reaction or a message from Lua. A parked agent costs ~0.01 ms. The limit is `dms_max_agents`
   (8).
6. **NM's world is the map cropped round the agent** (`world::CROP` = 32 m in x, y from the centre on an 8 m grid of
   NM's frame; refines 0018): faces reaching into it whole; the floor's squares the same as the whole world's (the
   crop's edges on their grid, past `TILED` coarse tiles whole). The crop moves when the pelvis is farther than 16 m
   from its centre. A spawn 175 → 29 ms, memory for a world a few MB.
   Golden after cropping: the same set outside tolerance (`kerb`, `lowwall`, `relax-1s`).
7. **Unloading the module**: GMod unloads and loads `gmsv_dms` again with every map, and what Rust's statics hold (NM's
   data, the maps read, worlds, tuning, Delirium's surfaces) isn't freed by `FreeLibrary`: ~65 MB a map change with
   agents. `gmod13_close` clears them explicitly (`data::unload`, `world::unload`, `tuning::unload`, `step`'s buffers);
   after it, Rust's live bytes are ~0.01 MB, memory grows ~8 MB a map change, as for the game without DMS.
8. **Listen server**: the callback goes on the object's environment (the server's `GetObjectEnvironment`), calls from
   another environment are counted (`DMS.FollowStats().foreign`) and ignored; on the bench, 0.
9. **Linux / a dedicated server**: not done: GMod's x64 server build is incompatible with the fork (ADR 0012), the
   user's decision is all tests in the game. Porting the fork to the server's ABI only on a separate request.

## Consequences

- The ≤ 0.15 ms/step budget holds for DMS's layer, not for NM itself (1–2 ms); threads, the limit and LOD keep the
  scale.
- On a listen server the agents' threads share cores with the game's client.
- An agent parked far away comes back in its behaviour (after `relax` usually lying: the room gets him up).
- Saves made before M7 carry `DMS_*` fields: a copy with someone else's agent is recognised, but "bound" without an
  agent isn't (such a Homer has to be spawned again).
- Bodies that vanished without the entity being removed (other than losing a joint) the module can't recognise.
