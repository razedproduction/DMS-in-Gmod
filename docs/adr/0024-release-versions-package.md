# 0024. The release: one version for the module and the addon, checks at start, the package

Status: accepted (2026-10-27). Refines 0005 (what's checked at start), 0006 and 0010 (where the GTA IV folder comes
from).

## Context

M8: the first release goes to players who build nothing. What a player installs comes in pieces that can drift apart:
the module (`gmsv_dms_win64.dll`), the Lua addon (which the Workshop can update on its own), the fork (`vphysics.dll`,
which Steam's file verification replaces), Homer's model (made by `dms-homer` on the player's machine, maybe by an
older release) and the GTA IV folder (a convar the player has to type). Before M8 the addon checked only that
`IPhysicsNM` exists; a mismatched module or model showed up as errors from the middle of the code.

Two things built on the developer's machine wouldn't work on a player's: `dms-homer` took its working folder from the
repository's path at build time (it would create `E:\GitStuff\…` there), and the Rust binaries needed
`VCRUNTIME140.dll`, which Garry's Mod (its CRT linked in) doesn't bring.

## Decision

1. **One version**: `Cargo.toml`'s `workspace.package.version` is the module's (`DMS.Version`) and `dms-homer`'s; the
   addon has `DMS_ADDON_VERSION` in `dms_init.lua`; `scripts\package.ps1` refuses to pack if they differ. The first
   release is 0.1.0.
2. **Checks at start** (`dms_init.lua`):
   - the module isn't installed, or **the module and the addon differ**: DMS doesn't start (the natives and tables of
     another release needn't match), the console and superadmins as they join are told what to install;
   - `DMS_Problems()`: no `IPhysicsNM`, or not `VBOX3D_NM_004`; no `homer.mdl`, or one made for another body; no GTA IV
     folder. Printed at start, told to superadmins, shown in the DMS window's Debug tab and by `dms_info`. DMS runs
     anyway: the problem itself says what fails (ADR 0005's behaviour is kept).
3. **The model's mark**: `dms-homer` writes `garrysmod/models/dms/homer_version.txt` (`model <n>`), where `n` is
   `dms_game::MODEL_VERSION`, raised whenever what `dms-homer` writes changes; the module exposes it as
   `DMS.HomerModelVersion`. The Delirium built in is `DMS.DeliriumVersion` (it's linked statically, so it can't differ
   from the module).
4. **The GTA IV folder**: `dms_game_dir`, else `garrysmod/data/dms/game_dir.txt`, which the release's installer writes
   (a path only, nothing of the game; `DMS_GameDir()`); `dms-homer` reads the same file. `dms-homer`'s working folder is
   the repository's `target\dms-homer` when run there, else `%TEMP%\dms-homer`.
5. **The C runtime linked in** (`.cargo\config.toml`, `+crt-static` for `x86_64-pc-windows-msvc`), as Garry's Mod's own
   binaries have it. No `target-cpu`: the binaries mustn't need AVX2 or FMA; `package.ps1` checks with `dumpbin` (and
   the build's `.pdb`) that `gmsv_dms_win64.dll` and `dms-homer.exe` import no VCRUNTIME and have FMA instructions only
   in the C runtime's maths (`sin`, `cos`, `atan2_fma`, `__remainder_piby2_fma3`, …), which the UCRT takes only on a CPU
   with FMA3 (`__use_fma3_lib`); the linked-in CRT brings them, as `ucrtbase.dll` has them. Their `ymm` code is
   runtime-dispatched (std, memchr); the fork's AVX2 is only Box3D's `wide8.c`, chosen by `b3HasAVX2`.
6. **The package** `dist\DMS-<version>-win64.zip` (`scripts\package.ps1`): `bin\` (`gmsv_dms_win64.dll`,
   `vphysics.dll`, `dms-homer.exe`), `addon\`, `install.ps1` (the player's: the fork with Steam's dll kept once as
   `vphysics.dll.original`, the module, the addon, `-Gta` → the folder's path and the model, `-Model`, `-Uninstall`),
   `install.md`, `LICENSE`, `THIRD_PARTY.md`, `licenses\` (VPhysics-Box3D, Box3D, Qhull, the Source SDK's LICENSE and
   thirdpartylegalnotices, which its licence requires with every copy), the release notes. Nothing of GTA IV, no
   development leftovers (`bench.txt`, recordings).
7. **The `dms-ctl` channel stays in the release** but closed: `dms_dev` is 0 and protected, it opens only from the
   one-off `data/dms/bench.txt` that `server.ps1` writes, on `127.0.0.1`, with a token (ADR 0012).
8. **Publishing** (a tag, a GitHub release, the Workshop's Lua part) only with the user's go-ahead; the Workshop doesn't
   take dlls, so the binaries and `install.ps1` are on GitHub Releases.

## Consequences

- A player sees one line about what's wrong instead of errors from the code; the FAQ in `docs/install.md` follows those
  lines.
- A new release that changes Homer's body raises `MODEL_VERSION`: players are asked to make the model again.
- The addon on the Workshop and the module from GitHub have to be updated together; until they are, DMS doesn't start
  and says so.
