# 0015. The VPhysics-Box3D fork is in the DMS repository through git subtree

Status: accepted (2026-10-26).

## Context

The VPhysics-Box3D fork with `IPhysicsNM` lived as a separate repository `razedproduction/VPhysics-Box3D` (a fork of
Asphaltian/VPhysics-Box3D) with a local copy `../VPhysics-Box3D`. Every physics change for the sake of matching the
Playroom (ADR 0008, 0014) went as two PRs into two repositories: the interface in the fork, its copy and use in the
module. Which fork commit a module commit needs wasn't written down anywhere; the golden comparison checked a build that
couldn't be restored from DMS's history.

## Decision

One repository: the fork is the folder `vphysics/` in `razedproduction/DMS-in-Gmod`, added with
`git subtree add --prefix vphysics https://github.com/razedproduction/VPhysics-Box3D.git main` (`43789dd`) **without
`--squash`**: the fork's and upstream's history is kept.

- The fork's submodules (`external/box3d`, `external/source-sdk-2013`, shallow) are in the root `.gitmodules` with the
  `vphysics/` prefix, at the same commits. `vphysics/.gitmodules` and `vphysics/.gitattributes` stay as upstream has
  them (the second works in its folder: `*.patch` with `eol=lf`).
- A fresh upstream: `git subtree pull --prefix vphysics https://github.com/Asphaltian/VPhysics-Box3D main`
  ([docs/vphysics.md](../vphysics.md)).
- Building: `scripts\build-vphysics.ps1` (the fork's CMake presets inside `vphysics/`), installing:
  `scripts\install.ps1 -Fork`.
- ADR 0008's rule doesn't change: the fork changes only for the sake of matching the Playroom, new methods with a new
  version of the interface.

Rejected: a submodule pointing at the fork (the same two PRs and updating the reference by hand), `--squash` (losing
the history and bisecting over upstream's commits, which has already been needed in the merge `9d5e14c`).

## Consequences

- A physics and module change is one PR; a DMS commit determines both the module and `vphysics.dll`.
- `razedproduction/VPhysics-Box3D` isn't updated any more (it can be archived), the local `../VPhysics-Box3D` isn't
  needed.
- The repository's history includes all of VPhysics-Box3D's. Its commits before the `subtree add` have paths without the
  `vphysics/` prefix, so `git log -- vphysics` doesn't show them: they're seen through the subtree merges' second
  parents (`git log cd0f52b^2`, `git log -- src/physics_nm.cpp`).
- The fork is MIT (`vphysics/LICENSE`), the rest GPL-3.0-only (ADR 0006); `vphysics.dll` is distributed under MIT.
- NM changes upstream doesn't need stay local: every `subtree pull` can conflict where the NM insertions are (sorting
  them out: docs/vphysics.md).
