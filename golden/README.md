# golden: the Playroom's references

The Playroom (`delirium-room` from the SDK) is the golden for DMS (ADR 0008). `tools/golden` builds it, records the
references and checks that DMS takes exactly the same NM inputs. Everything in `golden/out/` is made from the user's
game files and is **not committed** (ADR 0006, `.gitignore`); the repository holds only the scenarios and the tool.

## One command

```powershell
cargo run --release -p golden -- --game "E:\Downloads\TorrentDownloads\Grand Theft Auto IV" all
```

`--game` is remembered in `golden/game_dir.txt` (not in git either), so later it can be left out; or the variable
`DMS_GAME_DIR`. The SDK's folder is `..\delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)` by default, else `--sdk`.

| Command | What it does | Output in `golden/out/` |
|---|---|---|
| `build` | `cargo build --release -p delirium-room --locked --manifest-path <SDK>\Cargo.toml --target-dir target-playroom`, run from this repository; `git status` and the SDK's `Cargo.lock` before and after, any change is an error | `target-playroom\release\delirium-room.exe` |
| `dump` | the Playroom's `NM_DUMP` (`--game … --headless 0`: Homer = `m_y_pmanhat_01` after `getup::homer` + `stand_up`) against the build DMS makes (`tools/dms-game`); has to match **byte for byte** | `homer.nmdump`, `homer.ours.nmdump` |
| `links` | the links DMS derives from the idle clip (`DERIVED_LINKS`) are given to the Playroom through `NM_LINKS`; its 60 steps have to match the run without them bit for bit | `homer.links`, `links.*.frames` |
| `lead` | an SDK agent in LEAD built as in `dms_selftest` (the build from the dump, the Playroom room's CONCRETE floor: 1 m tiles with the pool's pit, `world::floor_tris`, seed 0.3, `bodyBalance`) against the Playroom's first second, bit for bit | `lead.dms.frames` |
| `run [names]` | the scenarios `golden/scenarios/*.txt` (all or the named ones) | `<name>.rec`, `<name>.log`, `<name>.frames` |
| `all` | everything in order | |
| `body [--no-game]` | Homer's body as `dms-homer` ports it (M3, [docs/homer.md](../docs/homer.md)): the build against `NM_DUMP` and the SDK's C API (1e-5), `homer.phy` against the build, in the game on the bench: frames, masses, inertia, a fall without NM | — |
| `body --fit [<mdl>]` | a body fitted to a GMod model (M11, ADR 0028; default male_07): Homer's build fitted here to the joints the module fitted it to (`DMS.FitJoints`) against the SDK's `nm_build_fit` on the Playroom's dump (1e-5), then on the bench the fork's native body (no NM) against it as Homer's, and a fall without NM | — |

The Playroom runs with `golden/out/` as its working folder (its own files land there, nothing is written into the SDK)
and with `ROOM_LOG=1` (the log in `<name>.log`). The Playroom is built without the SDK's `.cargo/config.toml` (it has
`target-cpu=x86-64-v3` with AVX2): the user's CPU has no AVX2.

## How the Playroom runs scenarios (important for M4)

The Playroom has **two** modes, and `--headless` **doesn't run `--script`** (only the windowed `scripted` system reads a
scenario):

1. **headless**: `--headless <N> [--press a,b,…] [--rec file]`: no window, deterministic. At step 0 it presses the
   `balance` button itself (`bodyBalance start=true`), then the buttons from `--press` in order; **at step 60 (1.00 s)
   it always shoots** the chest (`shot`: part 9 Spine2, the default weapon pistol from `weaponinfo.xml`, the
   Playroom's ShotTask). On stdout a frame a step: 21 parts × 7 floats (`x y z qx qy qz qw`) as hex bits (`{:08x}`),
   parts separated by ` | ` (`<name>.frames`). This is the exact reference (bit for bit).
2. **window**: `--script <file> [--rec file]`: the Playroom's window, commands by the **clock**
   (`time.elapsed_secs()`), NM steps in real time (1/60, an accumulator, at most 0.25 s a frame). Until the first
   command comes, the body is held in its pose and no steps run (`held`), so scenarios start at 3 s, after the window
   loads. Between commands everything is deterministic, but the step a command fires on can wander by ±1 frame: **each
   command's actual step is in the `.rec`** as a `# <t> …` line (t = step/60). Compare by those marks, not by the
   times in the scenario's file. `<t> !quit` closes the window; `golden` stops the Playroom if it hangs.
   `golden` starts the window with **`--safe`** (Safe Mode, since v0.2.1): without it the Playroom applies the user's
   `%APPDATA%\delirium\settings.txt`, which has behaviour switches too (`look`, `avoid`, `extra_reactions`, …), and on
   ordinary graphics the window runs at ~6 frames/s and loses steps (60 s of standing were only 1436 steps instead of
   3600). Safe Mode is the Playroom's defaults and minimal graphics; it doesn't touch the physics. The log's first line
   is then `# 0.00 room  Safe Mode: …` (not part of the command marks' comparison).

The get-up animation turned off: after 2 s lying, the Playroom lifts the body with the `get_up_fast` clip (that's not
NM). The `get-up` button cycles the modes `animation → muscles → off → learned`; two presses = `off`, so scenarios
start with `get-up` ×2 (`press=get-up,get-up` in headless).

## The scenario format `golden/scenarios/<name>.txt`

It's a Playroom `--script` file plus a mode line for `golden`:

```
# a comment (everything after # is ignored)
#! headless <steps> [press=button,button]     ← either
#! window [stage=<stage>]                      ← and then command lines (stage= → the Playroom's --stage, M5):
#! tol <check> ...                             ← the tolerances for Homer in FOLLOW (after the mode line)
<t, s> <command>
```

A line is parsed exactly as the Playroom does (`main`, `--script`): everything after the first `#` is dropped, the
line is trimmed, empty ones are skipped; then `split_once(' ')`: the time and the command are separated by **one
space** (not a tab), the time is an `f32`; a line whose time doesn't parse is silently skipped. Lines are sorted by
time stably (equal times in file order). A command fires in the first frame where `time ≤ elapsed time`; all the
frame's ripe commands run in order **before** that frame's NM steps. A command can't contain `#`. `#!` lines are
comments to the Playroom; only `golden` reads them. Commands (`scripted` in `delirium-room/src/main.rs`):

- **an NM message**: a line without `!`: `<message> [param=value …]`, as in the Playroom's console: `bodyBalance
  start=true`, `catchFall start=true`, `applyImpulse partIndex=9 hitPoint=x,y,z impulse=x,y,z`. `start` is a bool; the
  other params by their type (`nm_message_param_kind`): float/int as a number, bool `true/false/1/0`, vec3 `x,y,z`.
  It's sent to the agent through `msg::send` (as `nm_msg_send`) and releases `held`.
- `!button <button>`: a button of the Playroom's panel: the behaviours `balance drunk windmill pedal writhe rollup
  catchfall highfall stairs shot flinch brace look point lean relax stayup foetal grab overwall "STOP all"` (what they
  send is `Sim::lines`: `balance` = `bodyBalance start=true`, `relax` = `bodyRelax start=true`, `catchfall` =
  `catchFall start=true` …), and `shot` / `shot back`, `fall` (a new agent 3 m higher + `smartFall`), `push front` /
  `push back` (a plate at 1.2 m/s), `RESET pose`, `get-up`, `crowd`, `car`, `ice` and others.
- `!poke <part>`: `applyImpulse` of 8 N·s into a part from the camera (`eye` = (0, −4, 1.5)).
- `!shoot <part> [fatal]`: a shot (`Sim::fire`) into a part.
- `!grab <part> dx,dy,dz` / `!release`: pull a part to a point / let go.
- `!blast x,y,z`: a blast; `!strength <k>`: `agent.strength`; `!walk x,y`; `!pause`; `!quit`: quit.
- the other `!…` (`!cam`, `!crane`, `!photo`, `!screenshot`, `!hero…`, `!taxi_run`, `!portal …`): the camera,
  filming, Hero, cars, portals; not needed for references.

The scenarios now:

| Scenario | Mode | What |
|---|---|---|
| `stand-1s` | headless 60 | standing under `bodyBalance`, until the shot at step 60 |
| `relax-1s` | headless 60 | `balance`, then `relax` at step 0 |
| `shot` | headless 600 | standing, a shot in the chest at 1.00 s, 10 s |
| `stand` | window | standing under `bodyBalance` for 60 s |
| `push` | window | standing, after 2 s `!poke 9` (8 N·s into the chest) |
| `catchfall` | window | `fall` (3 m, `smartFall`) and at once `catchFall start=true` |
| `relax` | window | `bodyRelax` from standing |
| `kerb` | window, stage=kerb | standing, a 90 N·s push towards a 0.44 m kerb: he trips and falls |
| `lowwall` | window, stage=lowwall | standing, a 90 N·s push towards a 0.9 m wall: the wall stops him |
| `rooftop` | window, stage=rooftop | standing on a 4 m roof, a push towards the edge: armHang, highFall |
| `stairs` | window, stage=bigstairs | standing at the top of 6 m stairs, a push towards the steps |
| `blast` | window | standing, after 2 s `!blast` 1.5 m behind him: an 11 m flight, the explosion task (M6) |
| `nearmiss` | window | standing, two bullets 0.5 m from the head (`!miss`): he flinches and looks |
| `shotgun` | window | `!weapon 5`, a shotgun shot in the chest (`!shoot 9`): ShotTask with the shotgun's set |
| `death` | window | a fatal shot in the head: `STOP all`, `relax`, he goes limp |
| `getup` | window | `relax`, lies 2 s, gets up with the game's clip (`get_up_fast`), `bodyBalance` again |
| `tasks` | window | `!set tasks 1`, a shot: the task ends, Homer goes to the animation (stands) |

## The `--rec` format

The first line is the header `# game: frame t pelvis xyz |v| touches | ours: t com xyz |v|   (# lines: the log)`.
Then for every NM step:

```
- - - - - - - | <t> <com x> <com y> <com z> <|v com|> | p<k> <x> <y> <z> n<touches> g<gap> [<points>] | p<k> …
```

- the line is written after the NM step (`tick`), `t` = the step's number after it / 60, i.e. the first line is
  `0.017`;
- the "game" fields (`- - - - - - -`) are empty in the Playroom; the body's COM (x y z) and its speed (`doll.com()`);
- the two lowest parts by the z of their frame's centre (`frames()`, a stable sort by z): `p<k>` is the NM part's
  index, the centre x y z, `n<number>` the holding contacts (`held_world(k)`), `g<gap>` the smallest gap among them
  (9.000 if there are no contacts), in `[...]` the contact points `x+1.5,y,z` separated by spaces (the 1.5 shift in x is
  how the Playroom writes it); after the second, ` | crowd x y z` for each Homer of a crowd (scenarios have no crowd);
- all numbers are `{:.3}` (three digits after the point, `-0.000` possible), the separators exactly as in the template:
  ` | ` between groups, a space inside;
- `# <t> <text>` lines are the Playroom's log (`Sim::say`), `t` is `{:.2}` of the current step number / 60 (before the
  step the event affects): buttons pressed (`room  <button>`), messages sent (`ours  MSG <line>`), NM's feedback
  (`ours  FB <name> slot <n>`, anew each step), events (`room  shot …`, `room  poke …`). The first one is
  `# 0.00 weaponinfo.xml: N weapons` (not in headless).

Headless runs also give `.frames` (stdout): a line a step, 21 parts in NM's order, each with 7 values
`x y z qx qy qz qw` as `{:08x}` bits of an `f32`, a space inside a part, ` | ` between parts. This is the frame after
the step's `tick` and `rise`: what's compared bit for bit.

## Homer in FOLLOW against the Playroom (M4, ADR 0017)

`dms-ctl run` / `golden behaviour --source` run a scenario on Homer in FOLLOW on Box3D: a window scenario's commands go
to the module as `@<step> <command>`, the NM step before which the Playroom ran them (by the `#` marks of its
recording; `!quit` after its last step), `!poke` with the impulse from the Playroom's recording (its camera moves in the
window). The recording is written in the Playroom's tick order (a step's line is the state the step starts from; a new
agent's first step in the Playroom runs without physics).

Another physics engine won't repeat the Playroom's recording to the millimetre, and long scenarios are chaotic, so
Homer on Box3D is compared by **the scenario's tolerances**: the `#! tol` line (only `golden` reads it):

| Check | What has to hold |
|---|---|
| `steps=N` | the COM checks only over the first N steps |
| `com_max=M` | the COM is nowhere farther than M m from the Playroom's |
| `com_mean=M` | on average no farther than M m |
| `end_z=M` | the COM's height at the end within M m of the Playroom's |
| `fall` | the balancer gives up (`FB balance slot 8`) in both or neither, within 0.25 s |
| `down=Z:S` | the COM first below Z m within S s of the Playroom (or in neither) |
| `marks` | the commands on the same steps (the first words of each mark) |
| `said=TEXT:S` | the first log line starting with TEXT (`_` is a space) within S s of the Playroom's (the room layer's reactions) |

The Playroom's stages (`#! window stage=<name>`, ADR 0018): the Playroom runs with `--stage <name>`, the bench on the
`dms_stages` map (`tools/dms-map`, `server.ps1 start -Map dms_stages`), Homer at the stage's start (`room.start`). Stage
scenarios: `kerb`, `lowwall`, `rooftop`, `stairs` (an `applyImpulse` push of 90 N·s into the chest towards the kerb, the
wall, the edge, the steps).

`golden behaviour --source --lead`: the former comparison of the SDK's LEAD agent to three digits of the recording.
