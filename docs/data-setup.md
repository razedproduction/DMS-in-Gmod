# NM's data: your copy of GTA IV

Delirium needs the character's data from GTA IV. The addon doesn't have it and can't (ADR 0006): DMS reads it
**straight from your copy of the game** with the same `delirium-data` functions and in the same order as the Playroom
does with `--game` (ADR 0010). Nothing is copied or saved.

The PC version (the folder with `GTAIV.exe` and `pc\`) or Xbox 360 (the folder with `default.xex`; the Playroom puts
the xex key in `%APPDATA%\delirium\xex.key` the first time it runs with that game, and DMS only reads it) will do. PS3
isn't supported.

## In Garry's Mod

In the server's console (or as a superadmin):

```
dms_game_dir "D:\Games\Grand Theft Auto IV"
dms_data
dms_selftest
```

`dms_game_dir` is saved (`FCVAR_ARCHIVE`). While it's empty, DMS takes the folder from `garrysmod\data\dms\game_dir.txt`,
which the release's `install.ps1 -Gta` writes (the path only). The data loads once a run: Delirium keeps the character
in global state, so to change the folder, restart Garry's Mod.

What's read:

| What | From (PC) | From (Xbox 360) | What for |
|---|---|---|---|
| NM's character | `pc\data\naturalmotion\Fred.art` | `xbox360.rpf` → `data/naturalmotion/fred.xrt` | joints, masses, limits, self-collision (`nm_init_memory`) |
| Homer's body | `pc\models\cdimages\componentpeds.img` → `m_y_pmanhat_01.wft` | the same, `.xft` | the build through `delirium::ragdoll::Rig` |
| the standing pose and links | `pc\models\cdimages\playerped.rpf` → `player.wft`, `pc\anim\anim.img` → `move_player.wad` / `idle` | `xbox360.rpf`, `anim.img` | `derive_links`, `stand_up` |
| surfaces | `common\data\materials\materials.dat` | `common.rpf` | friction and restitution (`SURFACES`), the floor: the CONCRETE row |
| task tuning | `common\data\taskparams.txt` | `common.rpf` | the PC version has none: NM on its defaults, as in the Playroom |

On PC the game's files are little-endian: `delirium_data::LE` is set before reading, as the Playroom does.

## Without the data

`dms_selftest`, `dms_data`, `dms_nm_build` and `dms_nm_joints` without a GTA IV folder (or with a wrong one) say what's
missing and what to set. `DMS.LoadData(dir)` in Lua then throws an error with the same text.

## What matches the Playroom

`tools/golden` (`golden/README.md`) checks: the Homer build DMS makes matches the Playroom's `NM_DUMP` byte for byte;
the derived links give the Playroom the same run bit for bit; DMS's agent in LEAD (as in `dms_selftest`) runs bit for
bit with the Playroom's first second.
