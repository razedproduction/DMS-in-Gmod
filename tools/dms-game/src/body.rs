//! Homer's body as NM's own physics has it (`delirium::ragdoll::Ragdoll`, `delirium::nm::Agent::joints`): what a
//! Source ragdoll has to be for NM to drive it one to one (ROADMAP M3). Read off Delirium, never fitted.
//!
//! Every part's frame is the one `nm_agent_get_part_transform` reports and `nm_agent_set_part_state` takes (the
//! body's frame and its centre of mass). The part's capsule (or box) sits at that centre but not always along the
//! frame's Y: NM turns it by its own link (`Ragdoll::capsules`), so the shape keeps its own rotation in the frame.

use delirium::prelude::*;
use delirium::ragdoll::{Build, Ragdoll};

/// The rounding of NM's boxes (`delirium::ragdoll`'s `BOX_MARGIN`): a box part is a box `half - margin` swept by
/// a sphere of `margin`.
pub const BOX_MARGIN: f32 = 0.01;

/// A part's contact friction and elasticity as NM's physics has them (`delirium::ragdoll`'s `part_friction` and
/// `PART_ELASTICITY`), by the build part's name: they multiply the surface's (materials.dat) in its contacts.
pub fn part_material(name: &str) -> (f32, f32) {
    let friction = if name.ends_with("Foot") {
        1.0
    } else if name.ends_with("Hand") {
        2.25
    } else {
        0.75
    };
    (friction, 0.4)
}

/// A part's collision shape, in the part's frame, centred on its origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// Along `rot * Y`: a cylinder of `half` either way, capped by spheres of `radius`.
    Capsule { rot: Quat, radius: f32, half: f32 },
    /// A box of half extents `half` (rounding included) about `rot`'s axes.
    Box { rot: Quat, half: Vec3 },
}

#[derive(Clone, Debug)]
pub struct Part {
    /// NM's name (`nm_part_name`): the model's bone.
    pub name: &'static str,
    /// The game's bone it's on (`Char_Pelvis` ...).
    pub bone: String,
    pub parent: Option<usize>,
    /// kg, as NM's physics weighs it (fred.xrt's masses).
    pub mass: f32,
    /// The build's capsule radius and half length (`nm_build_part_info`; a box part's: its smallest extent, 0).
    pub radius: f32,
    pub half: f32,
    /// The frame in the build's standing pose (world, as `NM_DUMP` has it).
    pub pos: Vec3,
    pub rot: Quat,
    pub shape: Shape,
    /// NM's inertia tensor about the centre, in the part's frame (kg m^2).
    pub inertia: Mat3,
}

/// A part's joint to its parent (`delirium::ragdoll::JointInfo`, what `nm_agent_get_joint` reports).
#[derive(Clone, Copy, Debug)]
pub struct Joint {
    pub part: usize,
    pub parent: usize,
    pub hinge: bool,
    /// The pivot in the parent's frame and in the part's.
    pub anchor_parent: Vec3,
    pub anchor_child: Vec3,
    /// The joint's frame on the parent and on the part, each in its own frame.
    pub frame_parent: Quat,
    pub frame_child: Quat,
    /// A ball's lean X, lean Y and twist half ranges, a hinge's low and high (rad).
    pub limits: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct Body {
    pub parts: Vec<Part>,
    /// One per part but the root, in part order.
    pub joints: Vec<Joint>,
    /// NM's self-collision pairs (fred.xrt's), lower index first, sorted.
    pub pairs: Vec<(usize, usize)>,
}

impl Body {
    /// The body of `build` standing in `frames` (`Game::homer`'s). fred.xrt is loaded (as for `Game::homer`).
    pub fn new(build: &Build, frames: &[Transform]) -> Body {
        let doll = Ragdoll::new(build, frames, Vec3::ZERO);
        let caps: Vec<Transform> = doll.capsules().collect();
        let parts = build
            .parts
            .iter()
            .zip(frames)
            .enumerate()
            .map(|(k, (p, f))| {
                let turn = (f.rotation.inverse() * caps[k].rotation).normalize();
                let shape = match p.boxed {
                    Some(half) => Shape::Box { rot: turn, half },
                    None => Shape::Capsule { rot: turn, radius: p.radius, half: p.half },
                };
                let m = Mat3::from_quat(f.rotation);
                let t = doll.tensor(k).to_cols_array().map(|x| x as f32);
                Part {
                    name: delirium_data::nm_asset::PARTS.get(k).copied().unwrap_or(""),
                    bone: p.name.clone(),
                    parent: p.parent,
                    mass: doll.part_mass(k),
                    radius: p.radius,
                    half: p.half,
                    pos: f.translation,
                    rot: f.rotation,
                    shape,
                    inertia: m.transpose() * Mat3::from_cols_array(&t) * m,
                }
            })
            .collect();
        let agent = delirium::nm::Agent::new(build, Ragdoll::new(build, frames, Vec3::ZERO), Quat::IDENTITY, 0.3);
        let joints = agent
            .joints(build)
            .into_iter()
            .enumerate()
            .filter_map(|(k, j)| {
                j.map(|j| Joint {
                    part: k,
                    parent: j.parent,
                    hinge: j.hinge,
                    anchor_parent: j.anchor_parent,
                    anchor_child: j.anchor_child,
                    frame_parent: j.in_parent,
                    frame_child: j.in_child,
                    limits: j.limits,
                })
            })
            .collect();
        let mut pairs: Vec<(usize, usize)> = delirium::nmrs::SELF[build.nm]
            .get()
            .map_or(vec![], |p| p.iter().map(|&(a, b)| (a.min(b), a.max(b))).filter(|&(_, b)| b < build.parts.len()).collect());
        pairs.sort();
        pairs.dedup();
        Body { parts, joints, pairs }
    }

    /// NM's standing pose as an agent at `pos` (its feet's level) facing `yaw` has it (`nm_agent_create`'s
    /// `placed`): every part's frame.
    pub fn placed(&self, pos: Vec3, yaw: f32) -> Vec<(Vec3, Quat)> {
        let floor = self.parts.iter().map(|p| p.pos.z - p.radius - p.half).fold(f32::MAX, f32::min);
        let r = self.parts[0].pos.with_z(floor);
        let turn = Quat::from_rotation_z(yaw);
        self.parts.iter().map(|p| (pos + turn * (p.pos - r), turn * p.rot)).collect()
    }

    /// Where part `k`'s bone has its origin in the part's frame: the pivot of its joint to its parent (the root's:
    /// its centre). Source draws a ragdoll's child bone at a fixed offset from its parent's, so the bone has to
    /// turn about the joint's pivot; the body's centre of mass stays the part's centre.
    pub fn bone_offset(&self, k: usize) -> Vec3 {
        self.joints.iter().find(|j| j.part == k).map_or(Vec3::ZERO, |j| j.anchor_child)
    }

    /// The joint's pivot (world) and its frames on the parent and on the part (world), the bodies posed in
    /// `poses` (frame position and rotation per part).
    pub fn joint_world(&self, j: &Joint, poses: &[(Vec3, Quat)]) -> (Vec3, [Quat; 2]) {
        let (pp, pr) = poses[j.parent];
        let (_, cr) = poses[j.part];
        (pp + pr * j.anchor_parent, [(pr * j.frame_parent).normalize(), (cr * j.frame_child).normalize()])
    }
}
