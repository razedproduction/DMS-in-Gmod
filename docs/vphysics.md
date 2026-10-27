# The VPhysics-Box3D fork in `vphysics/`

`vphysics/` is a fork of [Asphaltian/VPhysics-Box3D](https://github.com/Asphaltian/VPhysics-Box3D) (a replacement of
GMod's `vphysics.dll` on Box3D) with the NM extension `IPhysicsNM` (`include/vphysics_nm.h`, now `VBOX3D_NM_004`). It
lives in this repository as a **git subtree** with its full history (ADR 0015): changes to the module and the physics
go in one PR, upstream comes with `git subtree pull`. The former separate repository `razedproduction/VPhysics-Box3D`
isn't updated any more.

The fork's licence is MIT, the file `vphysics/LICENSE` (the rest of the repository is GPL-3.0-only).

## What's where

- `vphysics/include/vphysics_nm.h`: the interface; the copy for the FFI is `module/src/ffi/vphysics_nm.h` (compare them
  at every change of the interface).
- `vphysics/src/physics_nm.*`: the implementation of `IPhysicsNM`; the other NM changes are in `physics_object.*`,
  `physics_constraint.*`, `physics_environment.cpp` (the step callback) and `cmake/patches/*.patch` (Box3D patches:
  joint armature, the elliptical cone, extra passes over joints with armature and NM bodies' contacts).
- The submodules `vphysics/external/box3d` (erincatto/box3d) and `vphysics/external/source-sdk-2013` (ValveSoftware)
  are in the root `.gitmodules` (shallow). `vphysics/.gitmodules` is upstream's file, git doesn't read it; it's kept so
  `subtree pull` doesn't conflict.
- `vphysics/.gitattributes` (`*.patch text eol=lf`) works inside `vphysics/` as is: without `eol=lf` the patches get
  CRLF on Windows and `git apply` won't apply them to Box3D.

## Building and installing

```powershell
git submodule update --init --depth 1   # once after cloning
.\scripts\build-vphysics.ps1            # -> vphysics\build\windows-x64\src\Release\vphysics.dll
.\scripts\install.ps1 -Fork             # the module + the addon + the fork -> GarrysMod\bin\win64\vphysics.dll
```

`build-vphysics.ps1` is `cmake --preset windows-x64` + `cmake --build --preset windows-x64-release` inside `vphysics/`
(CMake ≥ 3.25 and Visual Studio 2026 needed). After a `subtree pull` or a switch to another commit, build with
`-Clean`: an incremental build over another commit has already given a broken dll (heap corruption as a map loaded).
`install.ps1 -Fork` keeps Steam's dll as `vphysics.dll.original` on the first install and doesn't touch it after. The
fork's `build/` isn't in git (`vphysics/.gitignore`).

`windows-x64` only. Box3D turns AVX2 on only at run time (`wide8.c` with `/arch:AVX2`, chosen by cpuid,
`b3HasAVX2`); a CPU without AVX2 takes the ordinary path, so leave the build flags alone.

## The rule for changes

The fork changes only so that NM works one to one as in the Playroom (ADR 0008): first show the divergence (what
Delirium does, what the fork does), then fix it. New or changed methods come with a new version of the interface
(`VBOX3D_NM_004`…), keeping the old strings; copy the header into the module. Don't change anything unrelated to NM,
or every `subtree pull` will conflict.

## Changes for NM's physics in M4 (ADR 0017)

The interface didn't change (`VBOX3D_NM_003`); the behaviour for bodies and joints under `IPhysicsNM` did:

- `box3d-joint-armature.patch`: the armature's `Cdot0` is taken once a sub-step (before the forces are integrated), not
  on each relaxation pass: otherwise the second pass cancelled the armature's impulse and with > 1 pass the body blew
  up.
- `box3d-armature-iterations.patch`: `b3World_SetArmatureIterations`, extra Gauss–Seidel passes over the joints with
  armature only (only `IPhysicsNM` sets it) before every solve and relax iteration; the fork sets 8
  (`NM_ARMATURE_ITERATIONS`). Homer's almost rigid tree (armature up to 9 kg·m²) doesn't converge in one pass otherwise.
- `CPhysicsObject::GetBodyState` inside a step gives the velocity at the step's start (`CaptureNMStepStart`): the "core
  velocity" at the step callback already holds the step's gravity from the IVP controller (a body at rest read as
  falling at 0.23 m/s).
- Bodies under NM always take Box3D's continuous gravity (`FlushVelocity`), not the controller's `g·dt` kick at the
  step's start.

The check after every physics change: `golden body` and `golden behaviour --source` on the bench.

## Changes for NM's world in M5 (ADR 0018, `VBOX3D_NM_004`)

- New methods at the end of the vtable (the strings `VBOX3D_NM_003…001` still give the same object):
  `SetBodyMaterial(obj, μ, e)`, an NM part's material (the body's shapes get its friction and restitution,
  `userMaterialId` gets the tag `NM_BODY_MATERIAL` and the former surfaceprop); `SetSurfaceMaterial(surfaceprop, μ, e)`,
  NM's values for a surfaceprop (a table in `physics_nm.cpp`, for all environments).
- `MixFriction`/`MixRestitution` (`physics_environment.cpp`): an NM part × a surface with an NM material (or part ×
  part) is `μA·μB` without clamping and `min(eA·eB, 1)`, as NM's physics; everything else mixes surfaceprops as before.
- `CPhysicsConstraint::ApplyStiffnessBudget`: a joint between two NM bodies skips the stiffness budget, drift correction
  is Box3D's ceiling (¼ of the sub-step rate), ζ = 0.2 (`NM_JOINT_*`). The ankle under the body's weight stretches
  ~6 mm instead of ~9.
- `box3d-armature-contacts.patch` (ADR 0019): `b3Body_SetArmatureContacts`, a body's contacts aren't coloured but go to
  the overflow (the scalar path) and are solved again in every extra pass after the joints (`b3SolveArmatureContacts`;
  not added to `totalNormalImpulse`). The fork sets the flag in `CPhysicsObject::SetNMDriven`. The game's other
  contacts stay in colours, as upstream. The steady stretch of NM's joints (~5 mm) is the soft joint's springiness
  `F / (m_eff · ω²)`, not convergence (ADR 0019).

## A fresh upstream from Asphaltian

```powershell
git fetch origin
git switch -c vphysics-upstream origin/main
git subtree pull --prefix vphysics https://github.com/Asphaltian/VPhysics-Box3D main -m "vphysics: merge upstream Asphaltian/VPhysics-Box3D <commit>"
git submodule update --init --depth 1   # if upstream moved Box3D or the SDK
.\scripts\build-vphysics.ps1 -Clean
```

Without `--squash`: upstream's history comes in whole, as with `subtree add`. If upstream moved a submodule, the gitlink
`vphysics/external/box3d` updates with the merge; the root `.gitmodules` needn't change (only if upstream changed a URL
or added a submodule: then carry that into the root one with the `vphysics/` prefix).

The check after a merge: Box3D's patches applied (configure prints `Patching Box3D`, no `Failed to apply`), the game
gives `VBOX3D_NM_004` (`DMS.InterfaceVersion()`), `golden body` passes on the bench and the pivot and limit figures are
no worse than before (compare with a run before the merge). The PR as usual, its description with a table of conflicts.

### Conflicts of the NM changes

The NM changes sit where upstream is busy too, so conflicts are to be expected. How they were resolved in the merge
`9d5e14c` (formerly razedproduction/VPhysics-Box3D#2, commit `98e1130` in `vphysics/`'s history):

| File | Conflict | Resolution |
|---|---|---|
| `cmake/Box3D.cmake` | NM builds Box3D from a patched copy in the build tree; upstream removed `set(BOX3D_* OFF)` | `add_subdirectory("${BOX3D_DIR}" "${CMAKE_BINARY_DIR}/box3d-build" EXCLUDE_FROM_ALL)` (a separate binary dir: the sources are inside build), `set(... OFF)` removed as upstream |
| `physics_constraint.h` | NM: `m_flArmature`; upstream: new or renamed fields | upstream's fields + `m_flArmature` |
| `physics_constraint.cpp` (`CreateJoint`) | NM: `b3Joint_SetArmature` after creation; upstream: break thresholds, `QueueStiffnessUpdate` | the armature first, then upstream's block unchanged |
| `physics_object.h` | NM: `GetBodyState`/`ApplyDrive`/`SetInertiaTensor`; upstream: the stiffness budget | both groups of methods |
| `physics_environment.cpp` (`Step`) | NM: the step callback before `b3World_Step`; upstream: `UpdateStiffnessBudgets()`, `CaptureBulletSweeps()` | the NM callback first, then upstream's code (the budgets see joints NM recreated, the sweeps see the poses before the step) |

The general principle: upstream's code is taken as is, the NM insertions go beside it; in the step, the NM callback
comes first.

A merge can change behaviour without conflicts too. After `9d5e14c`, Homer's pivots in `golden body` went from 0.8 to
2.1–2.5 mm; bisecting found upstream's `IsContactActive` gate (`5ddb06c`: a contact without friction is dropped if the
bodies move apart). Fixed with the `m_bNMDriven` flag of `CPhysicsObject` (set by the `IPhysicsNM` methods that drive a
body), such bodies skip the gate (commit `4543837`); the interface didn't change. That's why `golden body` is a required
part of checking every merge.
