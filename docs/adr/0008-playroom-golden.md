# 0008. The Playroom is golden: we don't interfere with Delirium's work

Status: accepted (2026-10-21)

## Context

The project's goal is for Homer in GMod to behave **one to one as in the Playroom** (`delirium-room`). Any "fitting"
of NM to GMod (our own coefficients, filters, force scaling, our own body) makes the behaviour similar but not the
same, and every divergence then can't be told apart from an integration error.

The Playroom can do everything a reference needs:
- `NM_DUMP=<file>`: a dump of Homer's build (`m_y_pmanhat_01`, after `getup::homer` and `stand_up`: parts, masses,
  capsules, joints, frames in the standing pose);
- `--headless <steps>`: a run without a window;
- `--script <file>`: commands by time (`<t> <command>`);
- `--rec <file>`: a recording by frames: time, pelvis (xyz, |v|), touches, COM.

## Decision

1. **The Playroom is golden.** The correct behaviour is what the Playroom does with the same data. Not "what's right by
   physics" and not "what looks better".
2. **We don't interfere with Delirium's work.** Our code only passes data between Box3D and Delirium:
   - NM's inputs are the same as in the Playroom: the same `fred.xrt`, `taskparams.txt`, the same Homer build, the
     same NM step (1/60 s), the same messages with the same params;
   - NM's drives are applied as they are: without scaling, filters, limiters or additions of our own;
   - armature, pivots and joint limits exactly from `nm_agent_get_joint`;
   - no tuning of the behaviours' params of our own.
   **The only exception is gravity** (ADR 0007): the compensating force `m · (g_nm − g_src)`. Converting units
   (units ↔ metres) doesn't count as interfering.
3. **Where we diverge, we fix the physics, not Delirium.** Differences between Box3D and the Playroom's physics (the
   parts' inertia, the joint solver, armature, contacts) are removed in the VPhysics-Box3D fork (that's what it's for),
   showing the divergence before the fix.
4. **The comparison is automatic** (`tools/golden`):
   - **the body**: the build from `dms-homer` against the Playroom's `NM_DUMP`: parts, masses, radii, half-lengths,
     parents, the standing pose's frames, the joints' pivots, frames, limits and kind; tolerance 1e-5 (in effect, bit
     for bit), otherwise the import is wrong; for a Homer spawned in GMod, the parts' poses (≤ 1 mm, ≤ 0.5°), masses
     (exact) and inertia tensors (against Delirium's capsule inertia);
   - **the behaviour**: one scenario (`golden/scenarios/*.txt`) in the Playroom (`--headless --script --rec`) and in
     GMod (`dms_script`, a recording in the same format as `--rec`); the pelvis and COM trajectories, the touches and
     the flags (`balancing`, `fallen`) compared over time with tolerances set per scenario.
5. The Playroom's reference outputs come from the user's game data and don't go into the repository (`golden/out/` in
   `.gitignore`, ADR 0006); the repository holds only scenarios and tools.

## Consequences

- The Playroom is built from the SDK's folder without writing to it:
  `cargo build --release -p delirium-room --locked --manifest-path "<SDK>/Cargo.toml" --target-dir <ours>/target-playroom`.
- Every milestone changing the body or the loop (M3, M4, M5, M6) closes with a golden comparison, not "by eye".
- The difference between NM's 60 Hz and Source's 66 ticks is solved on our side (NM always gets a 1/60 step, as in the
  Playroom), the mechanism in M4.
- Proposals to "fix the behaviour" with NM's params are turned down; if Homer doesn't behave as in the Playroom, look
  for the divergence in the inputs or the physics.
