# Licences and credits

## DMS

DMS (the `gmsv_dms_win64.dll` module, the Lua addon, `dms-homer` and the other tools in this repository) is licensed
under the **GNU General Public License version 3 only** (GPL-3.0-only), see [LICENSE](LICENSE). The source of every
release is the repository's tag of that version: https://github.com/razedproduction/DMS-in-Gmod.

## Delirium

`gmsv_dms_win64.dll` and `dms-homer.exe` are built with **Delirium** (the `delirium`, `delirium-data` and `delirium-sdk`
crates), by PVCPC, licensed under GPL-3.0-only, linked in statically. Its source, the version these binaries are built
from: https://github.com/PVCPC/delirium-physics, tag `v0.2.2`.

Nothing else of Delirium's repository is in DMS: none of the Playroom's assets listed in its `CREDITS.md` (the taxi,
the sky, textures, the font, the Portal and Skate 3 code), and no trained networks (`getup.net`, `walk.net`,
`learned-preview/`): Delirium's "learned" get-up isn't ported, so Homer doesn't use `getup.net`.

## VPhysics-Box3D

`vphysics.dll` is VPhysics-Box3D (https://github.com/Asphaltian/VPhysics-Box3D) by Asphaltian, with DMS's `IPhysicsNM`
extension (the repository's `vphysics/` folder), licensed under the **MIT License**: `licenses/VPhysics-Box3D.txt`
(`vphysics/LICENSE`). It contains:

- **Box3D** by Erin Catto (https://github.com/erincatto/box3d), MIT License: `licenses/Box3D.txt`. DMS's changes are the
  patches in `vphysics/cmake/patches/`.
- **Qhull**, Copyright (c) 1993-1998 The Geometry Center, University of Minnesota: `licenses/qhull.txt`.
- Code from the **Source SDK 2013** (https://github.com/ValveSoftware/source-sdk-2013), Copyright (c) Valve
  Corporation, under the Source 1 SDK License: `licenses/SourceSDK2013-LICENSE.txt` and
  `licenses/SourceSDK2013-thirdpartylegalnotices.txt`. DMS is free of charge.

## Games

- **Grand Theft Auto IV** is Rockstar Games'. No files of it, and nothing made from them, come with DMS: Homer's model
  is made on your machine from your own copy, and NM's data is read from it as the game runs.
- **Garry's Mod** is Facepunch Studios'; the Source engine is Valve's.
- NaturalMotion and euphoria are NaturalMotion's. DMS and Delirium aren't affiliated with or endorsed by Rockstar Games,
  Take-Two Interactive, NaturalMotion, Facepunch Studios or Valve.
