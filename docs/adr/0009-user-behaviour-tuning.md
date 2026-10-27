# 0009. The player's tuning of behaviours over the Playroom's defaults

Status: accepted (2026-10-21). Refines ADR 0008, doesn't revoke it.

## Context

ADR 0008 forbids fitting NM: Homer's behaviour has to match the Playroom's. Yet the addon's user needs to be able to
change behaviours themselves (how strongly he reacts to a shot, stiffness, balance, duration) from the game's menu. If
that's mixed with the integration, tuning becomes a way to "hide" divergences from the Playroom.

## Decision

1. **By default, exactly the Playroom.** Without the player's settings NM gets the same data and params as in the
   Playroom; golden comparisons always run without tuning (`dms_tuning 0` forced in `tools/golden` and `dms-ctl run`).
2. **Tuning only through NM's public interface**, by the same channels the game itself uses:
   - the params of the behaviours' messages (`nm_msg_set_*` over the defaults; names, types, descriptions, defaults
     and "is it used" from `nm_message_param_*`);
   - the `taskparams` sets (values `<task> <set> <param>` replaced over the game's file).
   No changes inside Delirium, no scaling of drives, no formulas of our own.
3. **Tuning is a feature for the player, not an integration tool.** Divergences from the Playroom aren't fixed with
   tuning (ADR 0008 stands).
4. Settings are profiles (`garrysmod/data/dms/profiles/*.json`): the empty "Playroom" profile can't be changed, the
   others the user makes; a profile holds only the differences from the defaults. A "Reset to Playroom" button.
5. Only a superadmin can change the settings; they're applied on the server (the client UI sends `net` messages) and
   act from the next step (ADR 0003).

## Consequences

- A UI generated from the introspection of NM's messages is needed: new SDK params appear in the menu without code
  changes.
- Changed `taskparams` need the set reloaded in the SDK: check whether `nm_load_task_params` applies a second load; if
  not, `taskparams` tuning acts on new agents.
- The status and the telemetry show the active profile, to tell "Homer with tuning" from the reference one.
