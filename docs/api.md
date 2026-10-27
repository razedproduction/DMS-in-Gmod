# DMS's Lua API

Homer in FOLLOW mode: NM (Delirium) drives the `models/dms/homer.mdl` ragdoll, and the Playroom's reaction layer (ADR
0016, 0020) answers what the game does to him. Everything is on the server; only a superadmin can change anything
from the console and the menu.

## Quick start

```lua
-- A Homer from the spawn menu is bound by itself (dms_autoattach 1); your own: dms_homer or DMS_SpawnHomer(pos, yaw).
local c = dms.Attach(rag)          -- NM on the ragdoll + bodyBalance (the Playroom's "balance" button)
c:Shot(9, nil, "shotgun")          -- a shotgun shot in the chest: applyImpulse + the Playroom's ShotTask
c:Send("headLook", { start = true, pos = ply:EyePos() })
print(c:GetState().health)         -- 200 for a living Homer, damage is the game's ped::damage
dms.Detach(rag)                    -- an ordinary ragdoll
```

## Versions

`DMS.Version` (the module), `DMS_ADDON_VERSION` (the Lua addon), `DMS.DeliriumVersion` (the Delirium built into the
module), `DMS.InterfaceVersion()` (the fork's `IPhysicsNM`, `VBOX3D_NM_004` needed, `VBOX3D_NM_005` for GMod's ragdolls), `DMS.HomerModelVersion` (the body
`homer.mdl` has to be made for; `DMS_HomerModelVersion()` reads the model's mark). The module and the addon of
different releases don't start; `DMS_Problems()` lists what keeps Homer from working (the fork, the model, the GTA IV
folder), `dms_info` prints it all.

## `dms.*`

| Function | What it does |
|---|---|
| `dms.Attach(rag[, opts])` | NM on Homer (`opts.balance = false`: without `bodyBalance`). A controller, or `nil, reason`. The ragdoll has to be bound to NM's body (spawned through `dms_homer`, `DMS_SpawnHomer` or the spawn menu) |
| `dms.Detach(rag)` | NM off: armature 0, no drives, an ordinary ragdoll |
| `dms.Get(rag)` | Homer's controller with NM, or `nil` |
| `dms.All()` | the controllers of every Homer with NM |
| `dms.Messages()` | the names of all NM messages (behaviours) |
| `dms.MessageParams(name)` | a message's params: `{name, kind, used, doc, default}` (`used = false`: the port takes it but doesn't use it) |
| `dms.Reactions()` | `{buttons, extra, getup, weapons}`: the Playroom's buttons, additional reactions, get-up modes, weapons |

## The controller

| Method | What it does |
|---|---|
| `c:Send(msg[, params])` | an NM message as the Playroom's console takes it: `c:Send("bodyBalance start=true")` or `c:Send("bodyBalance", {start = true})`. A `Vector` is a game point (turned into NM's frame); for directions (`dir`, `normal`, `bulletVel`, `impulse`, `vel`, `fallOverEdge`) as is; a string `"x,y,z"` is NM's frame untouched |
| `c:React(line)` | a reaction of the Playroom's room layer (a `DMS.FollowReact` line, below) |
| `c:Shot(where[, dir][, weapon])` | a shot: `where` is a part 0–20 or a point (`Vector`), `weapon` one of `pistol deagle smg ak47 sniper shotgun rpg grenade molotov bat knife taser baton` |
| `c:Button(name)` | a Playroom button (`balance`, `catchfall`, `flinch`, `brace`, `relax`, `STOP all`, …, `shot`, `shot back`) |
| `c:Balance()`, `c:CatchFall()`, `c:Relax()`, `c:Stop()` | the buttons `balance`, `catchfall`, `relax`, `STOP all` |
| `c:Flinch([from])` | `upperBodyFlinch` from a point, or the `flinch` button |
| `c:Kill()`, `c:SetHealth(h)` | health 0 (death: `stopAllBehaviours` + `bodyRelax`) / any |
| `c:GetState()` | `{steps, parked, cost, origin, awake, asleep, balancing, fallen, pelvis, head, com, com_v, health, dead, rising, burning, shock, getup}` (m, tables `{x, y, z}`; `parked`: NM taken off by LOD, `cost`: NM's step, ms) |
| `c:GetParam(msg, param)` | a message param's description |
| `c:Origin()` | the start of NM's frame (m, the game's frame; moves with a teleport) |

## Hooks

| Hook | Arguments | When |
|---|---|---|
| `DMS_Attached` | `ent, id` | NM is on |
| `DMS_Detached` | `ent, why` | NM is off (`detached`, or NaN / blow-up: the reason) |
| `DMS_Fallen` | `ent` | NM's balancer gave up |
| `DMS_Feedback` | `ent, name, slot, value` | NM's feedback (after every NM step) |
| `DMS_Died` | `ent` | health 0: Homer went limp |
| `DMS_Reaction` | `ent, text` | a log line of the Playroom's room layer (`room  shot (pistol) at part 9 …`) |
| `DMS_Teleported` | `ent, metres` | the game moved the bodies (`PhysObj:SetPos`): NM's frame and its world moved with them |

## Lifecycle (M7, ADR 0021)

| Event | What DMS does |
|---|---|
| removal, `undo`, `gmod_admin_cleanup` | `OnRemove` takes the agent off before the bodies go |
| a map change, quitting | the `ShutDown` hook and `gmod13_close`: agents and the step callback off before the physics environment |
| a Lua restart (auto-refresh, `lua_openscript`) | the module and agents stay (`require` once a Lua state) |
| the duplicator, save/load (`gm_save`) | DMS's fields don't go into the copy (`PreEntityCopy`); the copy is bound and gets NM if the original had it (or with `dms_autoattach`) |
| freezing a part (physgun, a tool) | NM is parked while the part is frozen; the get-up clip's freezing is its own |
| picking up with the physgun or gravity gun during the get-up clip | the clip is broken off, the bodies unfrozen, a new agent balances on them |
| teleporting the bodies | NM's frame and the world's crop move, the agent and its state aren't reset |
| welds, ropes | NM stays (outside forces, like a grab) |
| a joint part is lost (`RemoveInternalConstraint`) | NM comes off (`DMS_Detached`, "joint is gone") |
| a panic in the module | a native: a Lua error; an agent's step: only that agent comes off (`DMS_Detached`, `panic: …`) |

LOD: farther than `dms_lod_distance` (3000 units, 0: off) from every player, Homer gets the `relax` button, after 1 s NM
is parked (no steps, drives or armature: an ordinary ragdoll; the agent, health and behaviour are kept) and comes back
when a player is near; lying still for `dms_lod_still` s (5, 0: off), he's parked until the bodies' first movement, a
reaction or a message. The limit is `dms_max_agents` (8): Homers beyond it stay ordinary ragdolls. Agents are stepped on
`dms_threads` threads (0: CPU cores less one).

## Reactions (`DMS.FollowReact(id, line)`, `dms_react`)

Points are `x,y,z` in metres, the game's frame. What the Playroom does on each: `module/src/follow/room.rs` (the port
of `Sim`).

| Line | The Playroom's reaction | What NM gets |
|---|---|---|
| `shot [part=k] [at= dir=] [weapon=] [fatal=1] [more=k,k]` | `fire` | the weapon's damage (`ped::damage`, `weaponinfo.xml`), the bullet's `applyImpulse` (`shottask::bullet`), ShotTask with the weapon's taskparams set (`weapon_set`); `more`: the other pellets |
| `blast at=` | `blast_on` | `applyImpulse` on each part within 5 m (15 m/s · mass · (1 − d/5)), damage 250 · (1 − d/5), the explosion task for 6 s |
| `burn` | `burn` | 25 damage a second; NM's OnFire task (`fire_reaction`) |
| `taser [part=k] [at=]` | `zap` | `electrocute start=true stunMag=0.4 largeMult=2` for 2.5 s |
| `melee damage= at= impulse= head=` | `land` (Hero's blow) | `applyImpulse`, the game's melee task: flinch (`MELEE`/`MELEE_PASIVE`) or balance |
| `nearmiss from= dir=` | `near_miss` | within 1.5 m of the head or chest: `bodyBalance`, `upperBodyFlinch`, `headLook`; lying: `bodyFoetal` (additional reactions); `dive_shot`: a dive |
| `car impulse= at= [id=] [player=1]` | `car_react` | damage `bump::damage`, the task `bump::react`: brace / rollup / balance / relax |
| `prop at= impulse=` | `obj_hit` | damage 0.6 per N·s (1–35), a limb injury (`injury`) |
| `thrown from= vel=` | `throw` | `headLook` (`look`), `upperBodyFlinch` (`avoid`) |
| `dropped` | `fall` | `smartFall` (additional reactions) |
| `button name=<button>` | `button` | the button's messages (`Sim::lines`); `STOP_all` is `STOP all` |
| `health value=`, `kill` | — | health; `kill`: a fatal shot |

The game is turned into reactions by itself (`addon/lua/dms/reactions.lua`): `EntityTakeDamage` (bullets → `shot`, one
tick's pellets → one `shot` with `more`, a blast → `blast`, fire → `burn`, a blow → `melee`, a stun gun → `taser`),
`PhysicsCollide` (a car → `car`, a prop → `prop`), `EntityFireBullets` (a miss → `nearmiss`), the physgun and gravity
gun (`thrown`, `dropped`). The push of a bullet or blast is the Playroom's `applyImpulse`, so Garry's Mod's own damage
force on Homer is zeroed.

## The Playroom's switches (convars, the Playroom's defaults)

| Convar | Default | What |
|---|---|---|
| `dms_tasks` | 0 | the game's tasks: NM → animation as a task ends |
| `dms_look`, `dms_avoid` | 1, 0 | looks at what's thrown / dodges it |
| `dms_getup` | 0 | getting up: 0 the game's clip, 1 muscles (NMPose along the clip), 2 doesn't get up, 3 the learned network (not ported) |
| `dms_extra_reactions`, `dms_extra_pick` | 0, 255 | v0.2.1's additional reactions and which of them (bits: windmill, curl up, curl up under fire, look at who hit him, tumble off a ledge, smartFall when dropped, dodge from above, brace on a wall) |
| `dms_fire_reaction` | 1 | OnFire while burning |
| `dms_dive_shot`, `dms_dodge_gap` | 0, 3 | a dive when shot at again |
| `dms_weak`, `dms_god` | 0, 0 | weak weapon sets, infinite health |

Bench and golden scenarios always run on the Playroom's defaults (only a scenario's `!set` changes them).

## Tuning (ADR 0009)

`dms_tuning 0|1`, `dms_tuning_profile <name>`; profiles are `garrysmod/data/dms/profiles/*.json`
(`{name, messages = {[message] = {[param] = "value"}}, taskparams = {["task set param"] = number}}`), differences from
the Playroom only; "Playroom" is empty and can't be changed. A profile's values are added to the messages that start a
behaviour or change its params, unless the line sets them itself; taskparams go over the game's file.
`dms_tuning_status`: the active values. The menu is the DMS window (ADR 0022): `dms_menu`, the key from `dms_menu_key`,
the DMS icon in the context menu (C) or "Utilities → DMS → Homer (Delirium)".

## The DMS window (ADR 0022)

Tabs: Homer (spawning, NM on/off the Homer you look at and every Homer, `dms_autoattach`, what this client draws,
colours, the window's key), Reactions (the Playroom's switches, get-up, additional reactions, try-outs), Tuning, Task
params, Performance (the M7 convars), Debug (the versions and problems, buttons for `dms_status`, `dms_selftest`,
`dms_data`, `dms_info`, `dms_tuning_status`, `dms_nm_messages`, `dms_nm_parts`, `dms_nm_joints`, an NM message line to
the Homer you look at, `dms_react_log`, the overlay). The help pane on the right explains what's under the cursor.

## The DMS tool (ADR 0023)

Toolgun → Tools tab → "DMS". The mode (`dms_tool_mode`): `attach`, `react`, `behaviour`, `target`, `inspect`, `kick`,
`throw`, `health`, `pin`; what the left and right buttons and R do is in the tool's hint and ADR 0023. Options:
`dms_tool_reaction`, `dms_tool_weapon`, `dms_tool_damage`, `dms_tool_impulse`, `dms_tool_behaviour`, `dms_tool_target`
(`look|point|lean|flinch|grab`), `dms_tool_speed`, `dms_tool_health`. Server: `dms_tool_everyone 0|1` (superadmins only
by default). Lua: `DMS_ReactAt(ent, kind, rest, hit, from, impulse)`, the Playroom's reaction at a point;
`DMS_SpawnHomerFor(ply, pos, yaw)`, a Homer with NM for a player.

## Console

| Command | What |
|---|---|
| `dms_homer` | Homer in front of the player, NM, `bodyBalance` |
| `dms_attach` | NM on the Homer the player looks at |
| `dms_detach` | NM off the Homer the player looks at |
| `dms_detach_all` | NM off every Homer |
| `dms_send <line>` | an NM message to the Homer under the crosshair (from the server console: to all) |
| `dms_react <reaction> [key=value …]` | a reaction of the Homer under the crosshair; without `at=`, the crosshair's point |
| `dms_status`, `dms_tuning_status` | the agents (NM's step cost, threads, LOD), tuning |
| `dms_info` | versions (module, addon, Delirium, `IPhysicsNM`, Homer's model) and what's wrong |
| `dms_game_dir "<folder>"`, `dms_data`, `dms_selftest` | your GTA IV folder, NM's data from it, NM's self-test (docs/data-setup.md) |
| `dms_react_log 1` | print the room layer's log |

## Client

`dms_eyes`, `dms_brows` (the Playroom's face: the eyes blink, look where he's hit or where he's going, squint in pain,
closed when dead; the brows: fright, alarm, pain), `dms_skeleton` (the Playroom's skeleton), `dms_overlay`
(superadmin: NM's capsules and boxes, the drives' forces and torques, the COM and its projection; green: balancing,
red: fallen), `dms_pedcol 0–4` (server: the Playroom's colour schemes, the model's skins).
