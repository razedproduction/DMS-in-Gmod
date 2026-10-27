# 0007. Gravity: sv_gravity stays, a force on every part makes up the difference

Status: accepted (2026-10-21)

## Context

- Delirium computes with the constant `delirium::ragdoll::GRAVITY = 9.8` m/s² (the C API can't change it, and the SDK
  mustn't be changed). Everything to do with weight is built on it: the muscles holding the body up (`oppose_gravity` in
  `nmdbm.rs`: torques from `-up · GRAVITY · m` along the chain from the support), balance, the fall prediction
  (`nmhf.rs`), hitting the ground.
- FOLLOW's drives come **without gravity** (`nm_agent_get_part_drive`: "gravity left out"): in the Playroom the host
  applies `-9.8 · m` to the parts itself, and the muscles are designed for exactly that load.
- In GMod `sv_gravity 600` u/s² = 600 · 0.0254 = **15.24 m/s²**. `sv_gravity` mustn't change: only Homer would then
  behave correctly, and the rest of the game (props, players, NPCs, vehicles) wouldn't.
- Doing nothing, Homer weighs 15.24 / 9.8 ≈ 1.56 times what NM expects: the muscles lack torque, the body sags and
  falls.
- NM's drives can't be scaled as a whole: only part of them depends on g (holding the weight), the rest are springs,
  damping, reactions; the support's reaction under a heavy body would stay different too.

## Decision

Every physics step, together with NM's drives, every Homer part gets through `IPhysicsNM::ApplyBodyDrive` (which acts
on all of Box3D's sub-steps) a compensating force

```
F_k = m_k · (g_nm − g_src)        g_nm = (0, 0, −9.8) m/s²,  g_src = the environment's gravity in m/s²
```

with `sv_gravity 600`: `F_k = m_k · 5.44 m/s²` upwards. The sum of outside forces on each part becomes exactly what
it is in the Playroom (`−9.8 · m_k` + NM's drive), so the joints, muscles, contacts and balance work as in the
Playroom, while `sv_gravity` and the rest of the world don't change.

- `m_k` is the part's mass by NM (`nm_build_part_info`); in Homer's model it's the physics object's mass (ROADMAP M3).
- `g_src` is taken from the game (`physenv.GetGravity()`, following `sv_gravity` changed on the fly) and passed to the
  module through the command queue (ADR 0003). A part with gravity turned off (`PhysObj:EnableGravity(false)`) gets
  `g_src = 0`.
- The mode (the convar `dms_gravity_mode`), finally chosen by measurements in M4:
  - `nm`: compensation always while the agent is attached: Homer lives in 9.8 entirely, an exact match with the
    Playroom.
  - `hybrid` (the default): compensation while the body has support or contact, or a behaviour that has to hold the
    weight is running; in free flight (no part touches the world) it's off, and Homer falls with the game's
    acceleration like every other object. Uniform gravity in flight doesn't change the parts' relative motion, so NM's
    muscles work the same in flight; it's switched on and off smoothly over 0.1 s, without jerks.
  - `off`: without compensation (for comparison).
- A detached or limp (`relax` without muscles) Homer gets no compensation: he's an ordinary game ragdoll.

## Consequences

- No change to the fork is needed: `ApplyBodyDrive` already applies the force continuously over the sub-steps. If the
  solver's accuracy is needed, the alternative is scaling the body's gravity in Box3D (`gravityScale`) with a new
  method of the interface.
- In the `nm` mode Homer falls slower than props (from 10 m: 1.43 s against 1.15 s) and presses on the support with a
  weight of 9.8 · m. In `hybrid` he falls as the world does, but on the support weighs as NM has it.
- NM's predictions in flight (the time to impact in `highFall`/`catchFall`) assume 9.8: in `hybrid` a long fall can
  get its timing slightly wrong; checked in M4.
- Debugging (M4) gets the compensation's output and a comparison of the pelvis height and the fall time with the
  Playroom.
