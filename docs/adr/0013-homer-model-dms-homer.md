# 0013. Homer's model: `dms-homer` builds the ragdoll from NM's body

Status: accepted (2026-10-22). Refines ADR 0004 (the physics object's frame) and 0006/0010 (the model is made on the
user's machine). The divergences from the fork in "Consequences" are removed by ADR 0014 (`VBOX3D_NM_003`).

## Context

M3: Homer's ragdoll in GMod has to be NM's body one to one (ADR 0008), so that in FOLLOW (M4) a Box3D part's state is
the NM part's state without conversions. What came up in the import:

- An NM part's frame (`nm_agent_get_part_transform`, `nm_agent_set_part_state`) is the `Ragdoll` body's frame (position
  = the capsule's centre), but **the capsule's axis isn't always its Y**: NM turns the shape with its link
  (`Ragdoll::capsules`); for the shin, neck and arms up to ~35°. Feet and hands in NM are rounded boxes (`BOX_MARGIN`
  1 cm), not capsules.
- A part's mass in NM comes from fred.xrt (`nmrs::MASSES`), not the fragment's mass; its inertia is the GTA fragment's
  tensor, **not diagonal** in the part's frame (up to ~45 % by norm for the arms).
- VPhysics-Box3D takes a body's mass and inertia from the `.phy` (the compact surface's centre of mass and "unit
  inertia" diagonal, the mass from the text); it keeps inertia **diagonal in the object's frame** (`m_Inertia`,
  `b3MassData` on the diagonal). It inflates the hull by `SHAPE_INFLATION` = 6.35 mm (like IVP's margin), welds
  vertices closer than 1 cm when compiling, at run time takes no more than 128 vertices. The game takes the solids in
  the order of their blocks (`RagdollAddSolid`). The fork builds a hinge only from a ragdoll constraint with one free
  axis.
- The fork's ball (`SetJointLimits`) is a twist/cone/plane ragdoll constraint, the twist axis picked by a heuristic
  (`SelectTwistAxis`); NM's twist is the joint frame's Z axis, the leans an ellipse in X/Y.
- Homer's PC `.wft` carries the skeleton and the ragdoll's parts, but its drawables are stubs; the ped's mesh is in
  `m_y_pmanhat_01.wdd` (head, upper, lower, skinned to the same skeleton), the textures in `.wtd` (DXT1).
  `delirium-data`'s mesh reader is made for Xbox 360's big-endian declarations.

## Decision

1. **NM's body** is `dms_game::body` (the crate shared with the Playroom-style reading): parts (NM's name, the GTA bone,
   parent, NM's mass, the standing pose's frame, the shape in the part's frame, the inertia tensor), joints
   (`Agent::joints`: pivots and frames on the parent and the child, limits, ball/hinge), self-collision pairs.
   `dms-homer`, the module (`bind.rs`) and `golden` use it.
2. **The model's bone = an NM part** (21, named `nm_part_name`, NM's hierarchy), its frame the NM part's frame in the
   standing pose of `nm_agent_create(pos, yaw 0)`; the model's origin is the floor under the pelvis. The shape lies in
   the frame turned.
3. **The `.phy`** is compiled by GMod's `studiomdl` (with the fork), then `dms-homer` rewrites it: the centre of mass =
   the bone's origin, unit inertia = the diagonal of NM's tensor / mass, the text: NM's exact masses, damping 0,
   inertia 1, ragdoll constraints (a hinge: one free axis, the one nearest NM's axis; a ball: all three; approximate
   limits, until the bind), only NM's self-collision pairs, solids in index order. The hulls are NM's capsule/box,
   shrunk by 6.35 mm, with a step ≥ 1.4 cm.
4. **The bind** (`DMS.BindHomer`, right after the spawn, in the spawn's pose): on every joint `SetJointAnchor` (NM's
   pivot) and `SetJointLimits` (NM's frames on the parent and the child, limits, hinge/ball). Spawning:
   `DMS_SpawnHomer` / `dms_homer`.
5. **The look**: the `.wdd` mesh (variant 000), reskinned to the 21 bones (a GTA bone → the part on it or on the nearest
   ancestor) and moved from the bind pose into the standing pose by the parts' rigid transforms; `.wtd` textures → VTF
   7.2. A PC vertex reader of our own in `dms-homer` (walking the drawable as `delirium-data` does). If the mesh doesn't
   work out: capsules.
6. Everything made from the game (SMD, `.mdl`, `.phy`, textures) stays only on the user's machine: the working files in
   `target/dms-homer/`, the model in `garrysmod/models/dms/`, the materials in `garrysmod/materials/models/dms/homer/`.
7. The check is `golden body`: the build against `NM_DUMP` and the SDK's C API (1e-5), the `.phy` against the build, in
   the game frames, masses, inertia, a throw without NM.

## Consequences

- Matching: the build (byte for byte), masses (exact), the centre of mass, the inertia diagonal, the poses of the 21
  bodies (≤ 0.6 mm, ≤ 0.1°), pivots hold in a fall (≤ 1.5 mm), the hulls within −3.1…+0.5 mm of NM's surface.
- **Not matching because of the fork** (by ADR 0008 fixed in the fork, with a new version of the interface, after the
  user agrees): (a) the inertia tensor's off-diagonal terms; (b) the ball's limits: a twist/cone/plane reading instead
  of twist about Z + an ellipse of leans (the hip goes ~15° past NM's ellipse in a fall). The plan: `VBOX3D_NM_003`
  with `SetBodyInertia` (the full tensor in the body's frame) and a ball with NM's semantics.
- Without the bind (a model spawned from the menu) the pivots sit in the parts' centres: Homer is spawned only through
  `dms_homer`.
- Soft tissue at the shoulders stretches: GTA's bind pose and NM's standing pose differ (linear skinning).

## Refinement (2026-10-22): the look as in the Playroom

The user's decision: GTA IV's look isn't needed, Homer has to look as in the Playroom. Point 5 is replaced: the visual
model is each part's NM capsule or box in its capsule frame (`delirium-room`'s `part_mesh`, without the fork's
inflation: this is the look, not the physics), the `part_color` colours ("mixed": left red, right blue, the rest grey,
translucent); the other `PED_COLORS` schemes are skins (`$texturegroup`). The `.wdd` mesh and `.wtd` textures aren't read
or exported (less derived game data), the PC mesh reader and the VTF converter are removed. The face (`face_view`) and
the skeleton overlay come in M6. The skin stretching at the shoulders is gone.

## Refinement (2026-10-22): the bone's origin in the joint's pivot

Point 2 is refined. Source draws a ragdoll without stretching: a child's bone is placed at a constant offset from the
parent's bone (`RagdollGetBoneMatrix`, `allowStretch` off), and with the bone's origin in the capsule's centre, bent
knees and elbows were drawn torn off the physics. Now a part's bone origin is the pivot of its joint with the parent
(`anchor_child`, `Body::bone_offset`; for the pelvis, its centre), the frame's rotation is the NM part's frame as
before, and the body's centre of mass stays in the part's centre (the centre of mass in the `.phy` is moved by
`-anchor_child`). `GetBodyState` (the centre of mass and rotation) doesn't change for NM. Along the way, a model without
the bind has its pivots in NM's joints, not in the parts' centres. `golden body` checks that Source draws the bones on
their bodies (≤ 5 mm after a fall; now 1.1 mm).
