# Architecture Decision Records

The decisions the project rests on. A new decision is a new file `NNNN-<slug>.md`; accepted ones aren't rewritten but
replaced by a new ADR saying "replaces NNNN".

| № | Decision | Status |
|---|---|---|
| [0001](0001-rust-module-sdk-rlib.md) | The module in Rust, the Delirium SDK linked as an `rlib` by path | accepted |
| [0002](0002-server-only-module.md) | A server binary module only | accepted |
| [0003](0003-no-lua-in-step-callback.md) | No Lua in the step callback, exchange through queues | accepted |
| [0004](0004-reference-body-first.md) | A 1:1 reference body first, real models later | accepted |
| [0005](0005-require-physics-nm.md) | Without `IPhysicsNM` the addon turns itself off | accepted |
| [0006](0006-gpl3-no-game-data.md) | GPL-3.0-only, GTA IV's data isn't distributed | accepted |
| [0007](0007-gravity-compensation.md) | `sv_gravity` stays, a force on every part makes up the difference from 9.8 | accepted |
| [0008](0008-playroom-golden.md) | The Playroom is golden, we don't interfere with Delirium's work (except gravity) | accepted |
| [0009](0009-user-behaviour-tuning.md) | The player's tuning of behaviours over the Playroom's defaults (refines 0008) | accepted |
| [0010](0010-nm-data-from-game-copy.md) | NM's data straight from the copy of the game with the Playroom's code (replaces part of 0006) | accepted |
| [0011](0011-golden-headless-and-window.md) | The Playroom's references: headless bit for bit, scenarios in the window (refines 0008) | accepted |
| [0012](0012-bench-game-and-dms-ctl.md) | The bench: the game without a window and the `dms-ctl` channel in the module (the dedicated server is incompatible with the fork) | accepted |
| [0013](0013-homer-model-dms-homer.md) | Homer's model: `dms-homer` builds the ragdoll from NM's body, the bind at spawn (refines 0004) | accepted |
| [0014](0014-fork-nm-003-inertia-ball.md) | The fork's `VBOX3D_NM_003`: the full inertia tensor and a ball with NM's semantics (refines 0013) | accepted |
| [0015](0015-fork-in-repo-subtree.md) | The VPhysics-Box3D fork is the `vphysics/` folder of this repository through git subtree | accepted |
| [0016](0016-playroom-reaction-layer.md) | The Playroom's reaction layer is carried over with the same code through the `delirium` crate (refines 0008) | accepted |
| [0017](0017-follow-on-box3d.md) | FOLLOW on Box3D: the agent on the `delirium` crate, fork changes for NM's physics, comparison by tolerances (refines 0007, 0008, 0011, 0016) | accepted |
| [0018](0018-nm-world-from-map.md) | NM's world from the map (BSP), props as obstacles, Box3D's shell, NM's contact materials: the fork's `VBOX3D_NM_004` (refines 0008, 0016, 0017) | accepted |
| [0019](0019-nm-contacts-in-armature-passes.md) | NM bodies' contacts in the fork's extra Gauss–Seidel passes; the remaining stretch is the soft joint's springiness (refines 0017, 0018) | accepted |
| [0020](0020-reaction-layer-port.md) | The Playroom's reaction layer in the module (`room.rs`), translating GMod's events, getting up held on the clip, tuning by profiles (refines 0009, 0016, 0017) | accepted |
| [0021](0021-lifecycle-performance.md) | Agents' lifecycle, the step in three phases with NM in parallel, LOD by parking, NM's world cropped, panics at the FFI boundary; no Linux (refines 0003, 0012, 0018) | accepted |
| [0022](0022-dms-window.md) | The DMS menu is a window of its own (`dms_menu`, a key, tabs, a help pane), a whitelist of the menu's convars, the profile changes at once (refines 0009) | accepted |
| [0023](0023-dms-tool.md) | The DMS tool for the toolgun: NM modes, reactions at a point, behaviours, a target, an inspector, a kick, a throw, health, pins (refines 0016, 0022) | accepted |
| [0024](0024-release-versions-package.md) | The release: one version for the module and the addon, checks at start (the fork, the model's mark, the GTA IV folder), the C runtime linked in, the package (refines 0005, 0006, 0010) | accepted |
