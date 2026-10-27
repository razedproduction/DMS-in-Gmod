# 0022. The DMS menu is a window of its own

Status: accepted (2026-10-27). Refines 0009.

## Context

M6's menu (ADR 0009) was a spawn menu tab "Utilities → DMS → Homer (Delirium)": a narrow `DForm` column where Homer, the
Playroom's switches, reaction try-outs, profiles, ~30 NM behaviours with dozens of params and taskparams went in one
long list; a param's documentation only in a tooltip with a delay. M7 added the convars `dms_max_agents`, `dms_threads`,
`dms_lod_distance`, `dms_lod_still`, which weren't in the menu. The user wants a separate window, like gwater2's
(`lua/menu`): its own window over the game, its own command and key, tabs on the left, its own style, an explanation of
what's under the cursor.

Along the way the bench found a race in `tuning.lua`: after `save` the server changed `dms_tuning_profile` through
`RunConsoleCommand` (applied the next frame), and the catalogue went out before, with the old profile; the menu saw the
mismatch, asked for the old profile and wiped the just-saved edits. It's the same loop fixed in M7 with the `filling`
flag, only from the server's side.

## Decision

- `addon/lua/dms/cl_menu.lua` is a window of its own (`DFrame`, `MakePopup`, a blurred background, a dark "glass" panel
  with one accent colour). It opens with the command `dms_menu` (again: closes), with a key from the client convar
  `dms_menu_key` (a `DBinder` in the Homer tab; doesn't fire in text boxes, the chat or the console), with the DMS icon
  in the context menu (C, `DesktopWindows`) and with a button in "Utilities → DMS" (the spawn menu tab itself stays only
  as a link to the window).
- Tabs on the left: Homer (spawning, taking NM off, `dms_autoattach`, client checkboxes, colours, the window's key),
  Reactions (the Playroom's switches, `dms_dodge_gap`, getting up, the additional reactions bit by bit
  `dms_extra_pick`, reaction try-outs), Tuning (on/off, profiles, search over behaviours, params, "run on Homer"), Task
  params (or a message that the PC version has none), Performance (the M7 convars). On the right, a help pane: the title
  and text of what's under the cursor (an NM param's description, its Playroom default, the mark "the port doesn't use
  it"; for the server's convars, their own help text), without delay; NM params keep their tooltip too.
- The `dms_tuning` protocol is the same (the catalogue, a behaviour's params on request, replies in 16 KB pieces once a
  tick) and extended: the catalogue carries `switches`, the values and help texts of the convars the menu changes (the
  client doesn't have the server's convars); `switch` takes only names from the `SWITCHES` whitelist in `tuning.lua`
  (before, any `dms_*`, including `dms_game_dir` and `dms_dev`), sets the value at once and answers with new `switches`.
- The server changes the profile and tuning with `ConVar:SetString`/`SetBool` (at once), not `RunConsoleCommand`; after
  each change, `DMS_TuningApply()` (saving again under the same name doesn't call the convar's callback).
- Filling widgets programmatically (`quietly`, the `filling` counter) never acts as the player's choice; the chosen
  behaviour isn't "selected" in the list but drawn by `ui.msg`, so refilling the list chooses nothing and the chosen one
  stays chosen (and between openings of the window). Sliders of server convars send their value 0.3 s after they stop.
  While idle, the window doesn't talk to the server: the catalogue is asked for only on opening, a behaviour's params
  once a session.

The code is written anew after the pattern of gwater2's layout, without copying (the project's licence is
GPL-3.0-only).

## Consequences

- All of DMS's settings in one place; new NM behaviours and params appear in the window without changes (as before).
- New server convars for the menu go into `SWITCHES` (`tuning.lua`) and into the right tab of `cl_menu.lua`.
- `dms_tuning`/`dms_tuning_profile` now change without a frame's delay; the console commands behave as before.
