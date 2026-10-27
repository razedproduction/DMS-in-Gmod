# 0010. NM's data straight from the copy of the game, with the Playroom's code

Status: accepted (2026-10-21). Replaces ADR 0006's point "the user puts their own files in `garrysmod/data/dms/`".

## Context

ADR 0006 assumed the user extracts `fred.xrt` / `character.json`, the build and `taskparams.txt` into
`garrysmod/data/dms/`. ADR 0008 requires NM's inputs to be exactly the Playroom's. The Playroom with `--game` takes them
from the game itself, and not only files:

- on PC it sets `delirium_data::LE` (little-endian `Fred.art`);
- it builds Homer from `componentpeds.img/m_y_pmanhat_01` through `delirium::ragdoll::Rig`;
- it derives the parts' links from the player's idle clip (`nmdbm::DERIVED_LINKS`, `getup::homer`): global state that
  affects the agent's behaviour but goes into neither the build dump nor `character.json`;
- it sets the standing pose (`Clips::stand_up`), the surfaces table from `materials.dat`, the CONCRETE floor;
- the PC version has no `taskparams.txt`: it runs on the defaults.

Through files in `data/dms/` (a build dump, `character.json`) the links and surfaces are lost: the agent would be
different.

## Decision

1. The data is read **from the game's folder** (`dms_game_dir`, a convar) when first needed; nothing is copied.
2. The reading is the shared crate `tools/dms-game`: the same `delirium-data` / `delirium` calls as `delirium-room`'s
   (`archive_read`, `getup::{key, plat, homer, Clips::stand_up}`, `load`), in the same order. The module and
   `tools/golden` use it.
3. The module hands the data to the SDK the standard way: `nm_init_memory` (the character), `nm_build_from_text` (the
   build dump: `Build::dump`, the `NM_DUMP` format), `nm_load_task_params`. The links and surfaces go into the same
   global tables the Playroom sets.
4. `tools/golden` checks the sameness with the Playroom: the dump matches `NM_DUMP` byte for byte, the links bit for bit
   by a Playroom run with `NM_LINKS`, the agent in LEAD bit for bit with the Playroom's first second.
5. PC and Xbox 360 are supported (the xex key is taken where the Playroom keeps it); PS3 isn't.

## Consequences

- A tool "make `character.json` from the game's files" isn't needed; `character.json` isn't used.
- The data loads once a process (that's how Delirium is made): changing `dms_game_dir` needs a restart.
- M3 (`dms-homer`) builds the model from the same `dms-game`.
