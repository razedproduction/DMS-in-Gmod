# 0017. FOLLOW on Box3D: the agent on the `delirium` crate, fork changes for NM's physics, comparison by tolerances

Status: accepted (2026-10-27). Refines 0007 (the default mode), 0008 (the behaviour comparison), 0011 (the commands'
steps), 0016 (what's carried over from the Playroom in M4).

## Context

M4: Homer in GMod stands and reacts as in the Playroom: NM in FOLLOW mode, the body 21 Box3D bodies. The first runs:
Homer fell after ~2 s, `golden behaviour` diverged from the Playroom on every scenario. The diagnosis is the
"prediction error": on every NM step its own physics (`step_follow` runs it on the same state) predicts the parts'
velocities a step later; Box3D's state at the next step against that prediction shows where the fork's physics
diverges from Delirium's (the telemetry `pred_err`, `limit_use`, `state_v`).

Found (the figures are the divergence from NM's prediction over a step, standing under `bodyBalance`):

| What | Delirium / Playroom | The fork / module before the fix | Divergence |
|---|---|---|---|
| joint armature | exact, in the articulated body's inertia matrix (Featherstone), `s/(1−s)` × the subtree's inertia | a soft constraint, 1 Gauss–Seidel pass; `Cdot0` was overwritten on every relaxation pass (with > 1 pass, a blow-up) | hips 0.5, feet 1.8 rad/s in the air |
| the velocity in `GetBodyState` | the state the step starts from | the "core velocity" with the step's gravity already added (the IVP controller) | a body at rest −0.23 m/s |
| the gravity of bodies without contact | continuous | a `g·dt` kick at the step's start: a body held by joints leaves the step with +0.06 m/s | +0.06 m/s |
| `bodyBalance`'s stabilising impulses (`nmdbm::stabilise`) | applied by its own physics (`doll.impulse`) | lost: they aren't in `doll.out` (nor in the SDK's C API) | Homer falls without them |
| `applyImpulse` | an impulse in the step's physics | the module read the params after `msg::send`, which resets one-off messages' state: the body got the default (0, −50, 0) N·s | the poke 6 times stronger and the other way |
| the recording and the room layer | the tick's physics runs before the behaviours, a new agent's first step without physics; `in_air` sees the state a step before the behaviours | line `n` = the state after `n` physics steps | a step's shift, `in_air` 0.04 s early |

## Decision

1. **The FOLLOW agent is on the `delirium` crate, as in the Playroom** (`module/src/follow.rs`): `Agent::step_follow`,
   `msg::send` messages with the Playroom's console parsing, `extras.outbox` forwarded (`extras_frame`), `in_air` (High
   Fall) with the Playroom's defaults: what the scenarios need. The impulses NM gives its own physics instead of
   `doll.out` go to Box3D's bodies with the same Delirium code: balance stabilisation (`nmdbm::stabilise` on the step's
   state, ×`k²` from `Character::stabilise`) and `applyImpulse` (`equalized_impulse`), as a force for one Source step.
2. **Time.** NM is always 1/60 (an accumulator over the 0.015 s physics steps; drives hold until the next NM step). A
   1/60 physics step and an NM step = the Source step were tried: no better. A scenario's recording is in the
   Playroom's order: a step's line is the state the step starts from; `in_air` by the previous step's velocity.
3. **Gravity defaults to `nm`** (the user's decision by measurements; refines 0007, whose default was `hybrid`): with
   `nm` all the scenarios are within tolerance; `hybrid` and `off` fall 0.15–0.2 s before the Playroom (`catchfall`),
   `off` also settles faster (`relax`) and sways the stance (`stand-1s` 0.21 m).
4. **Fork changes** (in one PR with the module, the interface unchanged: `VBOX3D_NM_003`):
   - `box3d-joint-armature.patch`: the armature's `Cdot0` is taken once a sub-step, before the forces are integrated;
   - `box3d-armature-iterations.patch`: `b3World_SetArmatureIterations`, extra Gauss–Seidel passes over the joints with
     armature only (only `IPhysicsNM` sets it), before every solve and relax iteration; the fork sets 8. The game's
     other joints and contacts aren't affected;
   - `GetBodyState` inside a step gives the velocity at the step's start (`CaptureNMStepStart` before the
     controllers);
   - bodies under NM always take Box3D's continuous gravity (as bodies in contact do).
5. **The behaviour is compared by the scenario's tolerances** (refines 0008): an engine other than the Playroom's won't
   repeat the recording to the millimetre, and long scenarios are chaotic (the Playroom's stance drifts 0.65 m over 60 s
   and takes steps). The scenario's `#! tol` line sets what's checked: the COM gap, the final height, the fall
   (`FB balance slot 8`), the settling time, the commands' steps. Window scenarios' commands run on the steps where the
   Playroom ran them (by its recording's marks, 0011).
6. Sleep: the NM doll's own sleep is kept (`set_body` wakes it; the module restores the mode until Box3D moves the
   body), Box3D's bodies fall asleep with it (the `sleep` event). Protection: a NaN or a blow-up (> 80 m/s, > 300 rad/s)
   takes NM off.

## Consequences

- Homer stands under `bodyBalance` for 60 s (COM z 0.942 m against the Playroom's 0.941), takes an 8 N·s poke (the COM
  gap 3.2 cm on average), `catchfall`/`relax` match the Playroom in time and height. 4 Homers at once: 67 ticks/s,
  ~1 ms of NM an agent.
- Known divergences: `relax-1s`: Homer settles faster (below 0.85 m at 0.72 s against 1.00 s): the supporting foot
  sinks 2–4 cm into Box3D's soft contact under the impact load (Box3D's contact stiffness is capped at 0.125 of the
  sub-step rate, ≈ 33 Hz; 8 sub-steps help partly but spoil the stance); Box3D's floor is 6.35 mm above the BSP plane
  (the fork's inflated hulls); the left clavicle and arm up to 0.19 rad/s while standing. That's the next step in the
  physics.
- The shot (`shot`) is the Playroom's ShotTask, M6; there too the `shot` behaviour's impulses (`shot_out.impulse`),
  which FOLLOW has to carry to the bodies as well.
- Every Delirium update: check whether new paths of "an impulse into its own physics" past `doll.out` appeared.
- A refinement of 0016 and the ROADMAP: in v0.2.2 the `stopAllBehaviours` in `catchfall` (0.67 s) isn't catchFall's
  outbox but the Playroom room layer's `in_air` (a fall faster than 6 m/s → `NMHighFall`); the outbox is forwarded as
  in the Playroom all the same.
