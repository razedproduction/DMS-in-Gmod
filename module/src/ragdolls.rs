//! GMod's ragdolls (ROADMAP M9, docs/adr/0025): the spikes' natives. A model's skeleton read from its `.mdl` (S1),
//! ValveBiped mapped onto NM's 21 joints and Homer's build fitted to them (`Build::fit`, as the SDK's Unity bridge
//! does, S2), the fitted body bound and driven in FOLLOW (S3), the model's bones posed from its parts (S4).
//!
//! Fitted bodies are kept by a key (the model's path) for the session; nothing of a model is written anywhere.

use std::collections::HashMap;
use std::ffi::{CStr, c_int};
use std::sync::{Arc, Mutex};

use delirium::prelude::*;
use delirium::ragdoll::{Build, Ragdoll};

use crate::data;
use crate::ffi::vphysics_nm::{self, IPhysicsObject};
use crate::follow::BodyDef;
use crate::lua::{self, Api, LuaState, TTABLE, native};

pub const NATIVES: &[(&CStr, lua::CFunction)] = &[
    (c"ModelSkeleton", model_skeleton),
    (c"FitModel", fit_model),
    (c"FitBody", fit_body),
    (c"FitDump", fit_dump),
    (c"FitJoints", fit_joints),
    (c"FitLead", fit_lead),
    (c"BindFitted", bind_fitted),
    (c"FollowFitted", follow_fitted),
    (c"PoseModel", pose_model),
];

/// Metres per Source unit.
const M: f32 = 0.0254;
/// `mstudiobone_t`'s size (studio versions 44 to 49).
const BONE_SIZE: usize = 216;

pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    /// Its frame in the model's space (units): the chain of its default transforms (`pos`, `quat` in its parent's),
    /// and the inverse of the `.mdl`'s `poseToBone`.
    pub bind: (Vec3, Quat),
    pub pose: (Vec3, Quat),
    /// Its physics object's index (`physicsbone`).
    pub physics: i32,
}

pub struct Skeleton {
    pub name: String,
    pub version: i32,
    pub checksum: i32,
    pub bones: Vec<Bone>,
}

impl Skeleton {
    pub fn find(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name.eq_ignore_ascii_case(name))
    }
}

fn i32_at(b: &[u8], at: usize) -> Result<i32, String> {
    b.get(at..at + 4).map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| format!("truncated at {at}"))
}

fn f32_at(b: &[u8], at: usize) -> Result<f32, String> {
    i32_at(b, at).map(|i| f32::from_bits(i as u32))
}

fn str_at(b: &[u8], at: usize) -> Result<String, String> {
    let s = b.get(at..).ok_or_else(|| format!("a name past the end ({at})"))?;
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len()).min(256);
    Ok(String::from_utf8_lossy(&s[..end]).into_owned())
}

/// The bones of a `.mdl` (`studiohdr_t`, versions 44 to 49: GMod's), their hierarchy and bind pose.
pub fn read_mdl(b: &[u8]) -> Result<Skeleton, String> {
    if b.len() < 408 || &b[..4] != b"IDST" {
        return Err("not a .mdl (no IDST)".into());
    }
    let version = i32_at(b, 4)?;
    if !(44..=49).contains(&version) {
        return Err(format!("studio version {version} (44 to 49 read)"));
    }
    let (count, index) = (i32_at(b, 156)?, i32_at(b, 160)?);
    if !(1..=256).contains(&count) || index < 0 || index as usize + count as usize * BONE_SIZE > b.len() {
        return Err(format!("{count} bones at {index}: not in the file"));
    }
    let mut bones: Vec<Bone> = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let o = index as usize + i * BONE_SIZE;
        let v = |at: usize| -> Result<Vec3, String> { Ok(Vec3::new(f32_at(b, o + at)?, f32_at(b, o + at + 4)?, f32_at(b, o + at + 8)?)) };
        let name = str_at(b, (o as i64 + i64::from(i32_at(b, o)?)) as usize)?;
        let parent = i32_at(b, o + 4)?;
        let parent = (parent >= 0).then_some(parent as usize).filter(|&p| p < i);
        let pos = v(32)?;
        let rot = Quat::from_xyzw(f32_at(b, o + 44)?, f32_at(b, o + 48)?, f32_at(b, o + 52)?, f32_at(b, o + 56)?).normalize();
        // poseToBone: rows of [R | t], the model's space into the bone's.
        let row = |r: usize| v(96 + r * 16);
        let (r0, r1, r2) = (row(0)?, row(1)?, row(2)?);
        let t = Vec3::new(f32_at(b, o + 96 + 12)?, f32_at(b, o + 96 + 28)?, f32_at(b, o + 96 + 44)?);
        let inv = Mat3::from_cols(r0, r1, r2);
        let pose = (-(inv * t), Quat::from_mat3(&inv).normalize());
        let bind = match parent {
            Some(p) => {
                let (pp, pq) = bones[p].bind;
                (pp + pq * pos, (pq * rot).normalize())
            }
            None => (pos, rot),
        };
        bones.push(Bone { name, parent, bind, pose, physics: i32_at(b, o + 172)? });
    }
    Ok(Skeleton { name: str_at(b, 12)?, version, checksum: i32_at(b, 8)?, bones })
}

/// ValveBiped's bones on NM's parts, in part order (`delirium_data::nm_asset::PARTS`); `None` where ValveBiped has
/// no bone of its own (NM's Spine1 to Spine3, spread between Spine and Neck as `NmCharacter.Joints` spreads them).
pub const VALVEBIPED: [Option<&str>; 21] = [
    Some("ValveBiped.Bip01_Pelvis"),
    Some("ValveBiped.Bip01_L_Thigh"),
    Some("ValveBiped.Bip01_L_Calf"),
    Some("ValveBiped.Bip01_L_Foot"),
    Some("ValveBiped.Bip01_R_Thigh"),
    Some("ValveBiped.Bip01_R_Calf"),
    Some("ValveBiped.Bip01_R_Foot"),
    Some("ValveBiped.Bip01_Spine"),
    None,
    None,
    None,
    Some("ValveBiped.Bip01_Neck1"),
    Some("ValveBiped.Bip01_Head1"),
    Some("ValveBiped.Bip01_L_Clavicle"),
    Some("ValveBiped.Bip01_L_UpperArm"),
    Some("ValveBiped.Bip01_L_Forearm"),
    Some("ValveBiped.Bip01_L_Hand"),
    Some("ValveBiped.Bip01_R_Clavicle"),
    Some("ValveBiped.Bip01_R_UpperArm"),
    Some("ValveBiped.Bip01_R_Forearm"),
    Some("ValveBiped.Bip01_R_Hand"),
];

/// Which NM part a ValveBiped bone is posed from (S4): its own, or for the spine the part whose span it's in.
const POSED: [(&str, usize); 21] = [
    ("ValveBiped.Bip01_Pelvis", 0),
    ("ValveBiped.Bip01_L_Thigh", 1),
    ("ValveBiped.Bip01_L_Calf", 2),
    ("ValveBiped.Bip01_L_Foot", 3),
    ("ValveBiped.Bip01_R_Thigh", 4),
    ("ValveBiped.Bip01_R_Calf", 5),
    ("ValveBiped.Bip01_R_Foot", 6),
    ("ValveBiped.Bip01_Spine", 7),
    ("ValveBiped.Bip01_Spine1", 8),
    ("ValveBiped.Bip01_Spine2", 9),
    ("ValveBiped.Bip01_Spine4", 10),
    ("ValveBiped.Bip01_Neck1", 11),
    ("ValveBiped.Bip01_Head1", 12),
    ("ValveBiped.Bip01_L_Clavicle", 13),
    ("ValveBiped.Bip01_L_UpperArm", 14),
    ("ValveBiped.Bip01_L_Forearm", 15),
    ("ValveBiped.Bip01_L_Hand", 16),
    ("ValveBiped.Bip01_R_Clavicle", 17),
    ("ValveBiped.Bip01_R_UpperArm", 18),
    ("ValveBiped.Bip01_R_Forearm", 19),
    ("ValveBiped.Bip01_R_Hand", 20),
];

/// NM's 21 joints on a ValveBiped skeleton in its bind pose (m, the model's space), as `NmCharacter.Joints` takes
/// them from a Unity humanoid: the pelvis, then each part's joint with its parent; the 4 spine joints spread evenly
/// from Spine to Neck; a missing Neck or Clavicle put between its neighbours. Also what was made up.
pub fn valvebiped_joints(s: &Skeleton, spread: [f32; 3]) -> Result<([Vec3; 21], Vec<String>), String> {
    let at = |k: usize| VALVEBIPED[k].and_then(|n| s.find(n)).map(|i| s.bones[i].bind.0 * M);
    let need = |k: usize| at(k).ok_or_else(|| format!("no {}", VALVEBIPED[k].unwrap_or("?")));
    let mut made = vec![];
    let (spine, head) = (need(7)?, need(12)?);
    let neck = at(11).unwrap_or_else(|| {
        made.push("Neck".to_string());
        spine.lerp(head, 0.85)
    });
    let (l_arm, r_arm) = (need(14)?, need(18)?);
    let mut clavicle = |k: usize, arm: Vec3, side: &str| {
        at(k).unwrap_or_else(|| {
            made.push(format!("Clavicle {side}"));
            neck.lerp(arm, 0.3)
        })
    };
    let (l_clav, r_clav) = (clavicle(13, l_arm, "L"), clavicle(17, r_arm, "R"));
    let j = [
        need(0)?, need(1)?, need(2)?, need(3)?, need(4)?, need(5)?, need(6)?,
        spine, spine.lerp(neck, spread[0]), spine.lerp(neck, spread[1]), spine.lerp(neck, spread[2]), neck, head,
        l_clav, l_arm, need(15)?, need(16)?,
        r_clav, r_arm, need(19)?, need(20)?,
    ];
    Ok((j, made))
}

/// Where Homer's Spine1, Spine2 and Spine3 joints are along his spine, from Spine0's joint (0) to the neck's (1),
/// as fractions of the way along his joints.
pub fn homer_spread(body: &dms_game::body::Body) -> [f32; 3] {
    let pivot = |k: usize| body.joints.iter().find(|j| j.part == k).map_or(body.parts[k].pos, |j| body.parts[k].pos + body.parts[k].rot * j.anchor_child);
    let p: Vec<Vec3> = (7..=11).map(pivot).collect();
    let legs: Vec<f32> = p.windows(2).map(|w| w[0].distance(w[1])).collect();
    let all: f32 = legs.iter().sum::<f32>().max(1e-4);
    [legs[0] / all, (legs[0] + legs[1]) / all, (legs[0] + legs[1] + legs[2]) / all]
}

/// A body fitted to a model.
pub struct Fitted {
    pub def: Arc<BodyDef>,
    /// The fitted build's frames, standing as the model's bind pose (m, the model's space).
    pub frames: Vec<Transform>,
    /// The model's bones posed from parts (S4): bone, part, the bone's frame in the part's (m).
    pub posed: Vec<(String, usize, Vec3, Quat)>,
    pub made: Vec<String>,
    /// The 21 joints it was fitted to (m, the model's space).
    pub joints: [Vec3; 21],
}

static FITS: Mutex<Option<HashMap<String, Arc<Fitted>>>> = Mutex::new(None);

fn fitted(key: &str) -> Result<Arc<Fitted>, String> {
    FITS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(key).cloned()).ok_or_else(|| format!("{key}: not fitted (DMS.FitModel)"))
}

/// Drops the fitted bodies (the module's unload, docs/adr/0021).
pub fn unload() {
    *FITS.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Homer's build fitted to a ValveBiped skeleton (`Build::fit` on Homer's standing frames), NM's spine joints spread
/// evenly from Spine to Neck (`even`, as `NmCharacter.Joints`) or as Homer's are along his spine.
pub fn fit(d: &data::Data, s: &Skeleton, even: bool) -> Result<Fitted, String> {
    let (joints, made) = valvebiped_joints(s, if even { [0.25, 0.5, 0.75] } else { homer_spread(&d.body) })?;
    let frames: Vec<Transform> = d.body.parts.iter().map(|p| Transform::from_translation(p.pos).with_rotation(p.rot)).collect();
    let (build, frames) = d.homer.fit(&frames, &joints);
    let body = dms_game::body::Body::new(&build, &frames);
    let posed = POSED
        .iter()
        .filter_map(|&(name, k)| {
            let b = &s.bones[s.find(name)?];
            let (pos, rot) = (b.bind.0 * M, b.bind.1);
            let f = frames[k];
            Some((b.name.clone(), k, f.rotation.inverse() * (pos - f.translation), (f.rotation.inverse() * rot).normalize()))
        })
        .collect();
    Ok(Fitted { def: Arc::new(BodyDef { build, body }), frames, posed, made, joints })
}

/// The body's height (m): its lowest point to its highest, its parts' capsules as they stand.
fn height(body: &dms_game::body::Body) -> f32 {
    let (lo, hi) = body.parts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
        let r = p.radius + p.half;
        (lo.min(p.pos.z - r), hi.max(p.pos.z + r))
    });
    hi - lo
}

native!(model_skeleton => model_skeleton_impl);
/// `DMS.ModelSkeleton(mdl)`: the `.mdl`'s bytes (`file.Read(model, "GAME")`) read: `{name, version, checksum,
/// bones = {{name, parent, pos, rot, physics, pose_gap, pose_turn}, ...}}`, `parent` 0-based (-1 for a root), `pos`
/// and `rot` the bind pose in the model's space (units, quaternion), `pose_gap` (units) and `pose_turn` (degrees) how
/// far `poseToBone`'s bind is from the chain of default transforms.
unsafe fn model_skeleton_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let s = read_mdl(unsafe { api.check_bytes(l, 1)? })?;
    unsafe {
        (api.createtable)(l, 0, 4);
        api.set_str(l, c"name", &s.name);
        api.set_number(l, c"version", f64::from(s.version));
        api.set_number(l, c"checksum", f64::from(s.checksum));
        (api.createtable)(l, s.bones.len() as c_int, 0);
        for (i, b) in s.bones.iter().enumerate() {
            (api.createtable)(l, 0, 7);
            api.set_str(l, c"name", &b.name);
            api.set_number(l, c"parent", b.parent.map_or(-1.0, |p| p as f64));
            api.set_vector(l, c"pos", &b.bind.0.to_array());
            api.set_vector(l, c"rot", &b.bind.1.to_array());
            api.set_number(l, c"physics", f64::from(b.physics));
            api.set_number(l, c"pose_gap", f64::from(b.bind.0.distance(b.pose.0)));
            api.set_number(l, c"pose_turn", f64::from(b.bind.1.angle_between(b.pose.1).to_degrees()));
            api.append(l, i + 1);
        }
        (api.setfield)(l, -2, c"bones".as_ptr());
    }
    Ok(1)
}

native!(fit_model => fit_model_impl);
/// `DMS.FitModel(key, mdl[, even])`: Homer's build fitted to the ValveBiped `.mdl` (its bytes; `even`: NM's spine
/// joints spread evenly, as the Unity bridge does, else as Homer's) and kept as `key` (the
/// model's path) for `DMS.BindFitted` and the rest: `{height, mass, homer_height, homer_mass, made, parts = {{name,
/// mass, radius, half, size}, ...}}` (m, kg; `made` the joints put between neighbours, `size` the part's scale).
unsafe fn fit_model_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let key = unsafe { api.check_string(l, 1)? };
    let s = read_mdl(unsafe { api.check_bytes(l, 2)? })?;
    let d = data::get().ok_or_else(|| format!("NM's data isn't loaded (DMS.LoadData): {}", data::how_to()))?;
    let even = unsafe { (api.toboolean)(l, 3) } != 0;
    let f = Arc::new(fit(&d, &s, even)?);
    let body = &f.def.body;
    let mass = |b: &dms_game::body::Body| b.parts.iter().map(|p| p.mass).sum::<f32>();
    unsafe {
        (api.createtable)(l, 0, 6);
        api.set_number(l, c"height", f64::from(height(body)));
        api.set_number(l, c"mass", f64::from(mass(body)));
        api.set_number(l, c"homer_height", f64::from(height(&d.body)));
        api.set_number(l, c"homer_mass", f64::from(mass(&d.body)));
        api.set_str(l, c"made", &f.made.join(", "));
        api.set_vector(l, c"spread", &homer_spread(&d.body));
        (api.createtable)(l, body.parts.len() as c_int, 0);
        for (k, p) in body.parts.iter().enumerate() {
            (api.createtable)(l, 0, 5);
            api.set_str(l, c"name", p.name);
            api.set_number(l, c"mass", f64::from(p.mass));
            api.set_number(l, c"radius", f64::from(p.radius));
            api.set_number(l, c"half", f64::from(p.half));
            api.set_number(l, c"size", f64::from(f.def.build.parts[k].size));
            api.append(l, k + 1);
        }
        (api.setfield)(l, -2, c"parts".as_ptr());
    }
    FITS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, f);
    Ok(1)
}

native!(fit_body => fit_body_impl);
/// `DMS.FitBody(key[, pos, yaw])`: the fitted body standing as `nm_agent_create` puts it at `pos` (m, its feet's
/// level) facing `yaw` (rad), as `DMS.HomerBody` has Homer's.
unsafe fn fit_body_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let at = if unsafe { (api.type_)(l, 2) } > lua::TNIL { Vec3::from_array(unsafe { api.check_vec3(l, 2)? }) } else { Vec3::ZERO };
    let yaw = unsafe { api.opt_number(l, 3, 0.0)? } as f32;
    let body = &f.def.body;
    unsafe { (api.createtable)(l, body.parts.len() as c_int, 0) };
    for (i, (p, (pos, rot))) in body.parts.iter().zip(body.placed(at, yaw)).enumerate() {
        unsafe {
            (api.createtable)(l, 0, 5);
            api.set_str(l, c"name", p.name);
            api.set_number(l, c"parent", p.parent.map_or(-1.0, |q| q as f64));
            api.set_number(l, c"mass", f64::from(p.mass));
            api.set_vector(l, c"pos", &pos.to_array());
            api.set_vector(l, c"rot", &rot.to_array());
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

native!(fit_dump => fit_dump_impl);
/// `DMS.FitDump(key)`: the fitted build as the Playroom's `NM_DUMP` writes one (`Build::dump`), for
/// `delirium-room <xrt> <build.txt>` and `dms-homer --fit`.
unsafe fn fit_dump_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    unsafe { api.push_str(l, &f.def.build.dump(&f.frames)) };
    Ok(1)
}

native!(fit_joints => fit_joints_impl);
/// `DMS.FitJoints(key)`: the 21 joints the body was fitted to, `x y z` (m) a line, for `dms-homer --joints`.
unsafe fn fit_joints_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    unsafe { api.push_str(l, &f.joints.iter().map(|j| format!("{} {} {}\n", j.x, j.y, j.z)).collect::<String>()) };
    Ok(1)
}

native!(fit_lead => fit_lead_impl);
/// `DMS.FitLead(key, steps[, message])`: the fitted body in LEAD (NM's own physics, the crate as the Playroom runs
/// it) on the Playroom's floor, `bodyBalance` from the start (or `message`): `{z = {pelvis height per step}, fell}`
/// (m), whether it stands without Box3D at all.
unsafe fn fit_lead_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let steps = unsafe { api.check_number(l, 2)? } as usize;
    let line = if unsafe { (api.type_)(l, 3) } > lua::TNIL { unsafe { api.check_string(l, 3)? } } else { "bodyBalance start=true".into() };
    let d = data::get().ok_or("NM's data isn't loaded")?;
    let z = lead(&d, &f.def.build, &f.def.body, steps, &line)?;
    let low = z.iter().copied().fold(f32::MAX, f32::min);
    unsafe {
        (api.createtable)(l, 0, 2);
        (api.createtable)(l, z.len() as c_int, 0);
        for (i, v) in z.iter().enumerate() {
            (api.pushnumber)(l, f64::from(*v));
            api.append(l, i + 1);
        }
        (api.setfield)(l, -2, c"z".as_ptr());
        api.set_bool(l, c"fell", low < z.first().copied().unwrap_or(0.0) * 0.6);
    }
    Ok(1)
}

/// A LEAD agent on `build` as the Playroom makes one (its room's floor, seed 0.3, `msg::send`), stepped `steps`
/// times at 1/60 s: the pelvis' height each step.
pub fn lead(d: &data::Data, build: &Build, body: &dms_game::body::Body, steps: usize, line: &str) -> Result<Vec<f32>, String> {
    let placed: Vec<Transform> = body.placed(Vec3::ZERO, 0.0).into_iter().map(|(p, q)| Transform::from_translation(p).with_rotation(q)).collect();
    let world = data::floor_collision(d, [0.0, 0.0, 0.0], false);
    let mut agent = delirium::nm::Agent::new(build, Ragdoll::new(build, &placed, Vec3::ZERO), Quat::IDENTITY, 0.3);
    let mut msgs = delirium::msg::State::default();
    let mut words = line.split_whitespace();
    let name = words.next().ok_or("no message")?;
    let ps: Vec<(&str, delirium::msg::Value)> = words
        .filter_map(|w| w.split_once('='))
        .map(|(k, v)| (k, if v == "true" { delirium::msg::Value::Bool(true) } else { delirium::msg::Value::Float(v.parse().unwrap_or(0.0)) }))
        .collect();
    delirium::msg::send(&mut agent, build, &world, &mut msgs, name, &ps).map_err(|e| format!("{name}: {e:?}"))?;
    let mut z = Vec::with_capacity(steps);
    for _ in 0..steps {
        agent.step(build, &world, 1.0 / 60.0, &[], &[]);
        z.push(agent.doll.frames().next().map_or(0.0, |f| f.translation.z));
    }
    Ok(z)
}

/// The 21 PhysObj of the table at `arg`.
unsafe fn objects(api: &Api, l: *mut LuaState, arg: c_int, count: usize) -> Result<Vec<*mut IPhysicsObject>, String> {
    if unsafe { (api.type_)(l, arg) } != TTABLE {
        return Err(format!("bad argument #{arg} (a table of {count} PhysObj in NM's part order expected)"));
    }
    (0..count)
        .map(|i| unsafe {
            (api.rawgeti)(l, arg, i as c_int + 1);
            let p = api.check_physobj(l, -1).map_err(|_| format!("bad argument #{arg} ([{}] isn't a PhysObj)", i + 1));
            (api.settop)(l, -2);
            p
        })
        .collect()
}

native!(bind_fitted => bind_fitted_impl);
/// `DMS.BindFitted(key, parts)`: `DMS.BindHomer` for a fitted body: its 21 PhysObj (a proxy ragdoll's, or the
/// fork's own bodies) get the fitted body's inertia, materials, pivots, frames and limits.
unsafe fn bind_fitted_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let parts = unsafe { objects(api, l, 2, f.def.body.parts.len())? };
    unsafe { crate::bind::bind_body(api, l, &f.def.body, &parts) }
}

native!(follow_fitted => follow_fitted_impl);
/// `DMS.FollowFitted(key, parts, yaw, floor[, room])`: `DMS.Follow` on a fitted body bound by `DMS.BindFitted`.
unsafe fn follow_fitted_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let parts = unsafe { objects(api, l, 2, f.def.body.parts.len())? };
    let yaw = unsafe { api.check_number(l, 3)? } as f32;
    let floor = Vec3::from_array(unsafe { api.check_vec3(l, 4)? });
    let room = unsafe { (api.toboolean)(l, 5) } != 0;
    let id = unsafe { crate::agents::start_body(parts, f.def.clone(), yaw, floor, room)? };
    unsafe { (api.pushnumber)(l, f64::from(id)) };
    Ok(1)
}

/// Source's angles (pitch, yaw, roll; degrees) of a rotation (`MatrixAngles`).
pub fn angles(q: Quat) -> [f32; 3] {
    let m = Mat3::from_quat(q);
    let (fwd, left, up) = (m.x_axis, m.y_axis, m.z_axis);
    let xy = (fwd.x * fwd.x + fwd.y * fwd.y).sqrt();
    let (pitch, yaw, roll) = if xy > 0.001 {
        ((-fwd.z).atan2(xy), fwd.y.atan2(fwd.x), left.z.atan2(up.z))
    } else {
        ((-fwd.z).atan2(xy), (-left.x).atan2(left.y), 0.0)
    };
    [pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees()]
}

native!(pose_model => pose_model_impl);
/// `DMS.PoseModel(key, parts)`: where the model's mapped bones are with the fitted body's 21 PhysObj as they are
/// now (S4): `{[bone name] = {pos = Vector-like (units), ang = {p, y, r}}}`.
unsafe fn pose_model_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let parts = unsafe { objects(api, l, 2, f.def.body.parts.len())? };
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    let poses = parts
        .iter()
        // SAFETY: PhysObj Lua handed over, alive for this call.
        .map(|&o| unsafe { nm.body_state(o) }.map(|s| (Vec3::from_array(s.position), Quat::from_array(s.rotation).normalize())).ok_or("GetBodyState failed"))
        .collect::<Result<Vec<_>, _>>()?;
    unsafe { (api.createtable)(l, 0, f.posed.len() as c_int) };
    for (name, k, at, turn) in &f.posed {
        let (p, q) = poses[*k];
        let (pos, rot) = ((p + q * *at) / M, (q * *turn).normalize());
        let [pitch, yaw, roll] = angles(rot);
        let name = std::ffi::CString::new(name.as_str()).unwrap_or_default();
        unsafe {
            (api.createtable)(l, 0, 2);
            api.set_vector(l, c"pos", &pos.to_array());
            api.set_vector(l, c"ang", &[pitch, yaw, roll]);
            (api.setfield)(l, -2, name.as_ptr());
        }
    }
    Ok(1)
}
