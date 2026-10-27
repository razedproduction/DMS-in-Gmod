# 0023. The DMS tool for the toolgun

Status: accepted (2026-10-27). Refines 0016, 0022.

## Context

In the DMS window (ADR 0022) reactions and behaviours are tried "on the Homer you look at", without a point of impact;
putting NM on a particular Homer, looking at his state, directing his gaze or hand at a point is possible only from the
console. The user asked for all of it in one toolgun tool.

## Decision

`addon/lua/weapons/gmod_tool/stools/dms_tool.lua` is one tool (the "DMS" category in the Tools tab), the mode chosen in
its panel (`dms_tool_mode`); the panel shows only the mode's options and is rebuilt when the mode changes from anywhere
(the list, the console, a bind). The left/right/R hints change with the mode.

| Mode | Left click | Right click | R |
|---|---|---|---|
| NM on / off | `dms.Attach` | `dms.Detach` | the Playroom's "balance" button |
| Reactions | the Playroom's reaction **at the point hit** (the weapon, damage and the blow's impulse from the panel) | `kill` | `health value=200` |
| NM behaviour | `<behaviour> start=true` (with the tuning profile, if it's on) | the "STOP all" button | "balance" |
| Target | a prop, a player or an NPC: Homer **follows it** while it moves; a point of the world: hold, and he follows the crosshair | choose a Homer (without a choice, the nearest) | `<message> start=false`, following stops |
| Inspector | choose a Homer: his state and NM's latest events on screen, the debug overlay only on him | clear the choice | the overlay on/off |
| Kick | hold: the force builds up; let go: the Playroom's blow (`melee`) into the part under the crosshair | — | — |
| Throw Homer | a new Homer in front of the player, thrown along the crosshair, then the reaction `dropped` | a new Homer standing at the crosshair's point | — |
| Health | `health value=<slider>` | `kill` | `health value=200` |
| Pins | a rope: a Homer part, then an anchor | remove his fastenings | a ballsocket of the part to the world at the point |

- The lines are the Playroom's (`delirium-room`): `headLook start=true pos= alwaysLook=true`, `pointArm start=true
  target=`, `leanInDirection start=true leanAmount=0.4 dir=`, `upperBodyFlinch start=true pos= newHit=true`,
  `grab start=true linegrab=true useLeft=true useRight=true dontLetGo=true pos1= pos2=` (a 0.6 m line across the way from
  Homer to the point). The points are turned into NM's frame through `Controller:Send`.
- Following a target: the point hit on a prop (in its local coordinates, moving with it) or a player's or NPC's eyes.
  The server checks the targets every 0.1 s and sends the line again **only if the point moved** (> 1 unit);
  `headLook` gets the target's velocity too (`vel`, as the Playroom's for a thrown thing), a repeated `upperBodyFlinch`
  without `newHit`. Following lives on without the tool in hand; it stops on R, a new target, the target's removal or NM
  coming off. The mode's HUD has a line from Homer to the target.
- Reactions at a point are a shared function `DMS_ReactAt` (`reactions.lua`), which `dms_react` now uses too; spawning
  Homer for a player (undo, cleanup, NM, `bodyBalance`) is `DMS_SpawnHomerFor` (`homer.lua`), which `dms_homer` uses.
- The inspector sends the state text to its owner 5 times a second, only while the tool is in hand in the "Inspector"
  mode; the overlay is limited to the chosen Homer through `ply.DMS_OverlayOnly` (`overlay.lua`).
- The list of behaviours and weapons in the panel comes from the window's catalogue (`DMS_MenuCatalogue`, the hook
  `DMS_Catalogue`, `cl_menu.lua`).
- Access: superadmins only, unless the server convar `dms_tool_everyone` is 1; spawning Homer is always superadmins only
  (as `dms_homer`).
- Fastenings are the game's constraints (`constraint.Rope`, `constraint.Ballsocket`). NM's world doesn't know about
  them: a pinned Homer no longer behaves as in the Playroom: it's a sandbox on top, and the mode's panel says so.

## Consequences

- Everything the tool does goes through the Playroom's inputs and the public API; NM isn't tuned, golden runs aren't
  affected.
- A new mode is a line in `MODES`, a branch in `TOOL:Act` and options in `BuildCPanel`.
- Not done: immortality for one Homer (`dms_god` is common to all; the module's room has no health per agent outside its
  table), "grabbing" a moving prop (`grab` goes to a point of the world, without `instanceIndex`).
