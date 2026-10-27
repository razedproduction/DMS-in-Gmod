# 0011. The Playroom's references: headless bit for bit, scenarios in the window

Status: accepted (2026-10-21). Refines ADR 0008 (the point about `--headless --script --rec`).

## Context

ADR 0008 counted on runs of `--headless N --script <scenario> --rec <file>`. By the Playroom's code (`main`,
`headless`, `scripted` in `delirium-room/src/main.rs`):

- `--headless` **doesn't run** `--script`: only the windowed `scripted` system reads a scenario;
- headless presses `balance` itself at step 0, the `--press` buttons next, and **always** shoots the chest at step 60;
  it prints the parts' poses as bits: an exact, deterministic reference;
- in the window a scenario's commands fire by the clock (`time.elapsed_secs()`), the steps run in real time; until
  the first command the body is held and no steps run.

The Playroom mustn't be changed (the SDK's folder is read-only).

## Decision

1. A scenario `golden/scenarios/*.txt` declares its mode with a line `#! headless <steps> [press=…]` or `#! window`.
2. **headless** for everything that fits "buttons at step 0 + a shot at step 60": the first second of standing, a shot
   in the chest, the first second of relax. The reference is `.frames` (bit for bit) and `.rec`.
3. **window** for the rest (standing 60 s, a push, `catchFall` from a height, `relax`): `--script --rec`, the first
   command at 3 s (after the window loads). Each command's actual step is taken from the `#` lines in the `.rec`; the
   comparison in M4 goes by those steps, not by the file's times.
4. The Playroom's get-up animation (not NM) is turned off in scenarios with the `get-up` button ×2 (the `off` mode).

## Consequences

- `dms_script` in GMod (M4) runs the commands on the steps from the reference `.rec`.
- Window runs open the Playroom's window for the scenario's length and run in real time.
- If the Playroom gets headless running of `--script`, the window scenarios move to headless.
