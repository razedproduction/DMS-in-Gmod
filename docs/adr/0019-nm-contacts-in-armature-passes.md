# 0019. NM bodies' contacts in the fork's extra Gauss–Seidel passes

Status: accepted (2026-10-27). Refines 0017 (the extra passes over joints) and 0018 (known divergences).

## Context

In M5 (0018) `relax-1s` diverged from the Playroom (0.77 s against 1.00), and so did `lowwall` and `stairs`. The feet
were no deeper than 4–7 mm in Box3D's floor, but Homer's joints stretched: the supporting leg's ankle up to 18–20 mm at
the peak, the wrists up to 2 cm (the telemetry `pivot_gap`). 0018's hypothesis: Gauss–Seidel not converging on "joint ↔
contact" at a light foot (77 kg of body on 1 kg of foot). The fork's extra passes (`b3SolveArmatureJoints`, 8 passes)
went over joints with armature only; the feet's contacts were solved once an iteration, in the graph's colours, in the
SIMD format (`contact_solver_wide.inl`).

## Decision

1. **The patch `box3d-armature-contacts.patch`** (over `box3d-armature-iterations.patch`):
   - `b3Body_SetArmatureContacts(body, flag)`: a body flag `b3_armatureContacts` (`b3Body` only). A contact with at least
     one flagged body isn't coloured when added to the graph (`b3AddContactToGraph`: touching starts, waking) but goes to
     the overflow: the scalar `b3ContactConstraint` path, which Box3D solves single-threaded between stages anyway. The
     contact is marked `b3_simArmatureContact`. When the flag changes, the body's touching contacts are recoloured.
   - `b3SolveArmatureContacts(context, useBias)`: in every extra pass **after the joints**: only overflow contacts with
     `b3_simArmatureContact`, one block of one contact each: with bias `b3PushContacts_Mesh` (the normal, soft
     correction), without bias the solve with friction, twist and rolling, but **not adding to `totalNormalImpulse`**
     (otherwise restitution, hit events and the contact force the game reads would swell).
   - The order and number of iterations for the rest of the game don't change: other contacts stay in colours, solved by
     the same code.
2. **The fork:** `CPhysicsObject::SetNMDriven` (the first call of any `IPhysicsNM` method on a body) sets
   `b3Body_SetArmatureContacts(body, true)`. `IPhysicsNM`'s methods are called outside `b3World_Step` (the step callback
   comes before it), the world isn't locked. The interface doesn't change (`VBOX3D_NM_004`).

## Consequences

The bench, A/B in one build (without the flag = M5):

| | M5 | with contacts in the passes |
|---|---|---|
| `relax-1s`, ankles, max (mm) | 18.0 / 19.8 | 15.0 / 10.5 |
| `stand-1s`, ankles, max / mean (mm) | 8.8 / 5.2 and 8.5 / 1.4 | 5.6 / 4.8 and 7.5 / 1.9 |
| `stand-1s`, the COM gap (m) | 0.065 | 0.040 |
| `push`, the COM gap max / mean (m) | 0.065 / 0.033 | 0.040 / 0.020 |
| `stairs` | outside (3.02 s against 3.78) | **within tolerance** (3.77 / 3.78 s, 5.18 / 5.22 s) |
| `lowwall` | falls, the COM gap 2.2 m | stands, the COM gap 0.123 / 0.066 m (tolerance 0.10 / 0.05) |
| `kerb` | within tolerance | the fall 2.53 / 2.50 s, going down 2.92 / 3.10 s: within tolerance; the final height 0.39 m (Playroom 0.20, tolerance 0.1) |
| `rooftop` | reactions within tolerance, lands lying | the same |
| `relax-1s` | 0.77 s | below 0.85 m after 1.00 s (at the recording's end the COM 1.8 cm above the Playroom's) |

`gm_flatgrass`: `stand` (60 s), `stand-1s`, `push`, `relax`, `catchfall`, `shot` within tolerance; `golden body` OK.

The passes cut the peaks and transients (standing, a poke, the stairs), but the **steady** stretch while standing hardly
changed (mean 5.2 → 4.8 mm). The cause isn't convergence: Box3D's soft joint (soft step) is GS on a regularised system
(CFM), and with any number of passes it converges to a solution with the springiness `C = F / (m_eff · ω²)`, where `ω`
is the joint's correction frequency (for NM, Box3D's ceiling, ¼ of the sub-step rate: 66.7 Hz with a 0.015 s step and 4
sub-steps), and `m_eff` the joint row's effective mass (a ~1 kg foot and a shin: fractions of a kg). Under half of
Homer's weight that's ~3–5 mm, under the body's weight on one leg ~6–9 mm: exactly what `pivot_gap` shows.

An experiment (not in the PR): a ceiling of ½ the sub-step rate for NM joints' stiffness (`NM_JOINT_HERTZ_FRACTION` 0.5
and the ceiling in `b3PrepareJoint` for joints with armature) together with the contact passes: the ankle's mean gap
7.8 → 0.6 mm, peak 15 → 5 mm; `gm_flatgrass` within tolerance except `relax-1s` (0.85 s), `stairs` within tolerance,
but `kerb` dropped out (the fall 2.87 s), `lowwall` goes 1.3 m, the `rooftop` landing lying. The joints' springiness is
confirmed, but stiffness alone doesn't fix the stages.

## The remaining divergences and the plan

- **`relax-1s`**: the time of going below 0.85 m depends chaotically on the physics (0.77 s in M5, > 1.00 s now, 0.85 s
  with stiff joints). The Playroom crosses the threshold exactly at the recording's last step (1.00 s), so "later" can't
  be measured in 60 steps. The cause of the divergence: the stance at the start still sags: COM 0.918 m at step 10
  against the Playroom's 0.938 (the joints' springiness + the soft contact), and a different starting pose leads to a
  different collapse. The plan: remove NM joints' springiness: stiffness above the ¼ ceiling (the experiment above,
  stability and all the stages to check) or a positional projection of NM joints' pivots after relaxation; then
  `relax-1s` and the stages again.
- **`kerb`, the final pose** (0.39 m against 0.20): the fall and going down match in time, Homer lies differently: a
  chaotic continuation of the same. The same plan.
- **`lowwall`** close to tolerance (0.123 / 0.066 m): Homer stands, as in the Playroom, but his steps along the wall are
  different. The same plan.
- **`rooftop`**, landing lying (COM 0.18 m against 0.92): unchanged; after highFall the pose before touching down has to
  be compared with the Playroom's (probably the same joint springiness at a ~5 m/s impact). The same plan, then a
  separate look.
