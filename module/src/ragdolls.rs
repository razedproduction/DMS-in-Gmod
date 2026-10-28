//! GMod's ragdolls (ROADMAP M9–M10, docs/adr/0025, 0027): a model's skeleton read from its `.mdl`, ValveBiped mapped
//! onto NM's 21 joints and Homer's build fitted to them (`Build::fit`, as the SDK's Unity bridge does), the fitted
//! body made by the fork natively (`VBOX3D_NM_005`/`_006`) in the model's ragdoll's environment, bound and driven in
//! FOLLOW as Homer is, the model's bones posed from its parts.
//!
//! Fitted bodies are kept by the model's path and its checksum for the session; nothing of a model is written
//! anywhere. The swap (`dms_attach`): the native body takes the ragdoll's pose and every part's velocity, and gives
//! them back to the model's objects on `dms_detach` (`DMS.PoseModel`).

use std::collections::HashMap;
use std::ffi::{CStr, c_int};
use std::sync::{Arc, Mutex};

use delirium::prelude::*;
use delirium::ragdoll::{Build, Ragdoll};

use crate::data;
use crate::ffi::vphysics_nm::{self, IPhysicsConstraint, IPhysicsObject, PhysicsNMBodyState, Version};
use crate::follow::BodyDef;
use crate::lua::{self, Api, LuaState, TTABLE, native};

pub const NATIVES: &[(&CStr, lua::CFunction)] = &[
    (c"ModelSkeleton", model_skeleton),
    (c"RagdollFit", ragdoll_fit),
    (c"FitDump", fit_dump),
    (c"FitLead", fit_lead),
    (c"FitJoints", fit_joints),
    (c"RagdollBody", ragdoll_body),
    (c"RagdollFollow", ragdoll_follow),
    (c"RagdollWarm", ragdoll_warm),
    (c"RagdollRemove", ragdoll_remove),
    (c"RagdollReport", ragdoll_report),
    (c"PoseModel", pose_model),
];

/// Metres per Source unit.
const M: f32 = 0.0254;
/// `mstudiobone_t`'s size (studio versions 44 to 49).
const BONE_SIZE: usize = 216;
/// Delirium's behaviours are tuned for about this height (m): a fitted body far from it gets a warning.
const TUNED_HEIGHT: f32 = 1.8;
const HEIGHT_WARN: f32 = 0.25;

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

/// The parts a ValveBiped ragdoll has no physics object for, each between two that it has (or that come before it
/// here) and how far along: where their pose comes from when NM takes over the ragdoll's.
const BETWEEN: [(usize, usize, usize, f32); 6] = [(7, 0, 9, 1.0 / 3.0), (8, 0, 9, 2.0 / 3.0), (10, 9, 12, 1.0 / 3.0), (11, 9, 12, 2.0 / 3.0), (13, 10, 14, 0.5), (17, 10, 18, 0.5)];

/// NM's 21 joints on a ValveBiped skeleton in its bind pose (m, the model's space), as `NmCharacter.Joints` takes
/// them from a Unity humanoid: the pelvis, then each part's joint with its parent; the 4 spine joints spread from
/// Spine to Neck by `spread`; a missing Neck or Clavicle put between its neighbours. Also what was made up.
pub fn valvebiped_joints(s: &Skeleton, spread: [f32; 3]) -> Result<([Vec3; 21], Vec<String>), String> {
    let at = |k: usize| VALVEBIPED[k].and_then(|n| s.find(n)).map(|i| s.bones[i].bind.0 * M);
    let need = |k: usize| at(k).ok_or_else(|| format!("no {} (not a ValveBiped skeleton)", VALVEBIPED[k].unwrap_or("?")));
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
    /// The `.mdl`'s checksum it was fitted from.
    pub checksum: i32,
    /// The fitted build's frames, standing as the model's bind pose (m, the model's space).
    pub frames: Vec<Transform>,
    /// The 21 joints it was fitted to (`Build::fit`'s input, in Homer's stance; m, the model's space).
    pub joints: Vec<Vec3>,
    /// The model's bones posed from parts (S4): bone, part, the bone's frame in the part's (m).
    pub posed: Vec<(String, usize, Vec3, Quat)>,
    /// What the player should know: joints made up, a height far from NM's tuning, bones posed only on the client.
    pub warnings: Vec<String>,
    pub height: f32,
    pub mass: f32,
}

static FITS: Mutex<Option<HashMap<String, Arc<Fitted>>>> = Mutex::new(None);

fn fitted(key: &str) -> Result<Arc<Fitted>, String> {
    FITS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(key).cloned()).ok_or_else(|| format!("{key}: not fitted (DMS.RagdollFit)"))
}

/// Drops the fitted bodies (the module's unload, docs/adr/0021).
pub fn unload() {
    *FITS.lock().unwrap_or_else(|e| e.into_inner()) = None;
    // The map's environment took the native bodies with it.
    *NATIVE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Each part's segment: from its joint to this child's (the leaves, feet, hands and head, turn with their parent).
const SEGMENT: [(usize, usize); 15] = [(1, 2), (2, 3), (4, 5), (5, 6), (7, 8), (8, 9), (9, 10), (10, 11), (11, 12), (13, 14), (14, 15), (15, 16), (17, 18), (18, 19), (19, 20)];

/// Homer's 21 joints as he stands (his build's frames): each part's pivot on its parent, the root's centre.
fn homer_joints(body: &dms_game::body::Body) -> Vec<Vec3> {
    (0..body.parts.len())
        .map(|k| body.joints.iter().find(|j| j.part == k).map_or(body.parts[k].pos, |j| body.parts[k].pos + body.parts[k].rot * j.anchor_child))
        .collect()
}

/// The model's joints (`bind`, its reference pose) put in Homer's stance (ROADMAP M10): each segment keeps the model's
/// length and takes the way Homer's goes, turned to the model's facing (its hips' line). NM's standing frames are what
/// it holds as the body's neutral, so a body fitted to ValveBiped's reference pose stood with its arms out and its head
/// forward. Also each part's turn from the reference pose to the stance, for the model's bones.
fn repose(body: &dms_game::body::Body, bind: &[Vec3; 21]) -> ([Vec3; 21], [Quat; 21]) {
    let h = homer_joints(body);
    let flat = |v: Vec3| v.with_z(0.0).normalize_or(Vec3::X);
    // Homer's hips' line onto the model's: a turn about Z.
    let face = Quat::from_rotation_arc(flat(h[4] - h[1]), flat(bind[4] - bind[1]));
    let mut at = *bind;
    for k in 1..21 {
        let p = body.parts[k].parent.unwrap_or(0);
        let way = (face * (h[k] - h[p])).normalize_or_zero();
        let len = bind[k].distance(bind[p]);
        at[k] = if way == Vec3::ZERO { at[p] + (bind[k] - bind[p]) } else { at[p] + way * len };
    }
    let mut turn = [Quat::IDENTITY; 21];
    for k in 1..21 {
        let p = body.parts[k].parent.unwrap_or(0);
        turn[k] = match SEGMENT.iter().find(|s| s.0 == k) {
            Some(&(_, c)) => {
                let (a, b) = ((bind[c] - bind[k]).normalize_or_zero(), (at[c] - at[k]).normalize_or_zero());
                if a == Vec3::ZERO || b == Vec3::ZERO { turn[p] } else { Quat::from_rotation_arc(a, b) }
            }
            None => turn[p],
        };
    }
    (at, turn)
}

/// Homer's build fitted to a ValveBiped skeleton (`Build::fit` on Homer's standing frames), NM's spine joints spread
/// as Homer's are along his spine (ADR 0025), the skeleton put in Homer's stance first ([`repose`], ADR 0027).
pub fn fit(d: &data::Data, s: &Skeleton) -> Result<Fitted, String> {
    let (bind, made) = valvebiped_joints(s, homer_spread(&d.body))?;
    let (joints, turn) = repose(&d.body, &bind);
    let frames: Vec<Transform> = d.body.parts.iter().map(|p| Transform::from_translation(p.pos).with_rotation(p.rot)).collect();
    let (build, frames) = d.homer.fit(&frames, &joints);
    let body = dms_game::body::Body::new(&build, &frames);
    let posed: Vec<(String, usize, Vec3, Quat)> = POSED
        .iter()
        .filter_map(|&(name, k)| {
            let b = &s.bones[s.find(name)?];
            // The bone as the stance has it: turned with its part about the part's joint.
            let (pos, rot) = (joints[k] + turn[k] * (b.bind.0 * M - bind[k]), (turn[k] * b.bind.1).normalize());
            let f = frames[k];
            Some((b.name.clone(), k, f.rotation.inverse() * (pos - f.translation), (f.rotation.inverse() * rot).normalize()))
        })
        .collect();
    let (height, mass) = (height(&body), body.parts.iter().map(|p| p.mass).sum::<f32>());
    let mut warnings: Vec<String> = made.iter().map(|m| format!("no {m} bone: put between its neighbours")).collect();
    if (height - TUNED_HEIGHT).abs() > HEIGHT_WARN {
        warnings.push(format!("{height:.2} m tall: Delirium's behaviours are tuned for about {TUNED_HEIGHT} m and work worse this far from it"));
    }
    let missing: Vec<&str> = VALVEBIPED.iter().flatten().copied().filter(|n| s.find(n).and_then(|i| (s.bones[i].physics >= 0).then_some(())).is_none()).collect();
    if missing.len() > 6 {
        warnings.push(format!("{} of NM's bones have no physics object here: they're drawn on the client only", missing.len()));
    }
    Ok(Fitted { def: Arc::new(BodyDef { build, body, fitted: true }), checksum: s.checksum, frames, joints: joints.to_vec(), posed, warnings, height, mass })
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

native!(ragdoll_fit => ragdoll_fit_impl);
/// `DMS.RagdollFit(model, mdl)`: Homer's build fitted to the ValveBiped `.mdl` (`model` its path, `mdl` its bytes),
/// kept by the path and the `.mdl`'s checksum (a changed model is fitted again): `{height, mass, homer_height,
/// homer_mass, checksum, cached, ms, warnings = {...}, bones = {[bone] = part}, parts = {{name, mass, radius, half,
/// size}, ...}}` (m, kg; `bones` the model's bones NM's parts pose, 0-based parts). A Lua error for a model that isn't
/// ValveBiped.
unsafe fn ragdoll_fit_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let key = unsafe { api.check_string(l, 1)? };
    let bytes = unsafe { api.check_bytes(l, 2)? };
    let d = data::get().ok_or_else(|| format!("NM's data isn't loaded (DMS.LoadData): {}", data::how_to()))?;
    let started = std::time::Instant::now();
    let checksum = i32_at(bytes, 8).unwrap_or(0);
    let kept = fitted(&key).ok().filter(|f| f.checksum == checksum);
    let cached = kept.is_some();
    let f = match kept {
        Some(f) => f,
        None => {
            let f = Arc::new(fit(&d, &read_mdl(bytes)?)?);
            FITS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, f.clone());
            f
        }
    };
    let body = &f.def.body;
    unsafe {
        (api.createtable)(l, 0, 10);
        api.set_number(l, c"height", f64::from(f.height));
        api.set_number(l, c"mass", f64::from(f.mass));
        api.set_number(l, c"homer_height", f64::from(height(&d.body)));
        api.set_number(l, c"homer_mass", f64::from(d.body.parts.iter().map(|p| p.mass).sum::<f32>()));
        api.set_number(l, c"checksum", f64::from(f.checksum));
        api.set_bool(l, c"cached", cached);
        api.set_number(l, c"ms", started.elapsed().as_secs_f64() * 1000.0);
        (api.createtable)(l, f.warnings.len() as c_int, 0);
        for (i, w) in f.warnings.iter().enumerate() {
            api.push_str(l, w);
            api.append(l, i + 1);
        }
        (api.setfield)(l, -2, c"warnings".as_ptr());
        (api.createtable)(l, 0, f.posed.len() as c_int);
        for (bone, k, _, _) in &f.posed {
            api.set_number(l, &std::ffi::CString::new(bone.as_str()).unwrap_or_default(), *k as f64);
        }
        (api.setfield)(l, -2, c"bones".as_ptr());
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
    Ok(1)
}

native!(fit_dump => fit_dump_impl);
/// `DMS.FitDump(model)`: the fitted build as the Playroom's `NM_DUMP` writes one (`Build::dump`).
unsafe fn fit_dump_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    unsafe { api.push_str(l, &f.def.build.dump(&f.frames)) };
    Ok(1)
}

native!(fit_joints => fit_joints_impl);
/// `DMS.FitJoints(model)`: the 21 joints Homer's build was fitted to (`Build::fit`'s input, as `nm_build_fit` takes
/// them), a line `x y z` (m) each, for `golden body --fit` to fit Homer's build itself: the dump can't carry a fitted
/// build (its sizes).
unsafe fn fit_joints_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let lines: Vec<String> = f.joints.iter().map(|j| format!("{:.9} {:.9} {:.9}", j.x, j.y, j.z)).collect();
    unsafe { api.push_str(l, &lines.join("\n")) };
    Ok(1)
}

native!(fit_lead => fit_lead_impl);
/// `DMS.FitLead(model, steps[, message])`: the fitted body in LEAD (NM's own physics, the crate as the Playroom runs
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

/// The game flag that keeps an entity's objects off each other (Source's `FVPHYSICS_NO_SELF_COLLISIONS`): the native
/// bodies take their owner's entity, so the game keeps them off the model's own objects; among themselves the fork's
/// pairs (`SetBodyCollisionPairs`) decide first.
const NO_SELF_COLLISIONS: u16 = 0x8000;

/// A fitted body made by the fork (`VBOX3D_NM_005`+): its 21 bodies in part order and their joints (null for the
/// root).
#[derive(Clone)]
pub struct NativeBody {
    pub objects: Vec<*mut IPhysicsObject>,
    pub joints: Vec<*mut IPhysicsConstraint>,
}

// SAFETY: only used on the game's main thread, where the physics and Lua both run.
unsafe impl Send for NativeBody {}

static NATIVE: Mutex<Option<(u32, HashMap<u32, NativeBody>)>> = Mutex::new(None);

fn native(handle: u32) -> Result<NativeBody, String> {
    NATIVE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|(_, m)| m.get(&handle).cloned()).ok_or_else(|| format!("no native body #{handle}"))
}

/// A part's state: frame (m, world) and its centre's velocity and spin.
#[derive(Clone, Copy)]
struct PartState {
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    spin: Vec3,
}

impl PartState {
    /// The rigid motion of this part carried to a point.
    fn at(&self, p: Vec3) -> Vec3 {
        self.vel + self.spin.cross(p - self.pos)
    }
}

/// The model's ragdoll objects handed over by Lua: `{{name = bone, obj = PhysObj, pos = its origin (units)}, ...}`.
unsafe fn model_objects(api: &Api, l: *mut LuaState, arg: c_int) -> Result<Vec<(String, *mut IPhysicsObject, Vec3)>, String> {
    if unsafe { (api.type_)(l, arg) } != TTABLE {
        return Err(format!("bad argument #{arg} (a table of {{name, obj, pos}} expected)"));
    }
    let mut out = vec![];
    for i in 1.. {
        unsafe { (api.rawgeti)(l, arg, i) };
        if unsafe { (api.type_)(l, -1) } != TTABLE {
            unsafe { (api.settop)(l, -2) };
            break;
        }
        let top = unsafe { (api.gettop)(l) };
        let item = (|| unsafe {
            (api.getfield)(l, top, c"name".as_ptr());
            let name = api.check_string(l, -1);
            (api.settop)(l, -2);
            (api.getfield)(l, top, c"obj".as_ptr());
            let obj = api.check_physobj(l, -1);
            (api.settop)(l, -2);
            (api.getfield)(l, top, c"pos".as_ptr());
            let pos = api.check_vec3(l, -1);
            (api.settop)(l, -2);
            Ok::<_, String>((name?, obj?, Vec3::from_array(pos?) * M))
        })();
        unsafe { (api.settop)(l, -2) };
        out.push(item.map_err(|e| format!("bad argument #{arg} ([{i}]: {e})"))?);
    }
    Ok(out)
}

/// Every part's state from the model's ragdoll as it is now: the parts whose bones have an object from them (each
/// bone's bind-pose offset to its part), the rest between their neighbours ([`BETWEEN`]), or off their parent as in
/// the bind pose.
fn parts_from_model(f: &Fitted, objects: &[(String, *mut IPhysicsObject, Vec3)]) -> Result<Vec<PartState>, String> {
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    let n = f.def.body.parts.len();
    let mut parts: Vec<Option<PartState>> = vec![None; n];
    for (name, obj, origin) in objects {
        let Some((_, k, at, turn)) = f.posed.iter().find(|p| p.0.eq_ignore_ascii_case(name)) else { continue };
        // SAFETY: a PhysObj of the model's ragdoll Lua handed over.
        let s = unsafe { nm.body_state(*obj) }.ok_or("GetBodyState failed")?;
        let bone_rot = Quat::from_array(s.rotation).normalize();
        let rot = (bone_rot * turn.inverse()).normalize();
        let pos = *origin - rot * *at;
        let (com, v, w) = (Vec3::from_array(s.position), Vec3::from_array(s.velocity), Vec3::from_array(s.angular_velocity));
        parts[*k] = Some(PartState { pos, rot, vel: v + w.cross(pos - com), spin: w });
    }
    if parts[0].is_none() {
        return Err("the model's ragdoll has no pelvis object".into());
    }
    // Part b's frame as it is in the bind pose relative to part a's, carried by a's state.
    let rel = |a: &PartState, ka: usize, kb: usize| -> PartState {
        let (fa, fb) = (f.frames[ka], f.frames[kb]);
        let rot = (a.rot * fa.rotation.inverse() * fb.rotation).normalize();
        let pos = a.pos + a.rot * (fa.rotation.inverse() * (fb.translation - fa.translation));
        PartState { pos, rot, vel: a.at(pos), spin: a.spin }
    };
    for &(k, a, b, t) in &BETWEEN {
        if parts[k].is_some() {
            continue;
        }
        if let (Some(pa), Some(pb)) = (parts[a], parts[b]) {
            let (x, y) = (rel(&pa, a, k), rel(&pb, b, k));
            parts[k] = Some(PartState { pos: x.pos.lerp(y.pos, t), rot: x.rot.slerp(y.rot, t).normalize(), vel: x.vel.lerp(y.vel, t), spin: x.spin.lerp(y.spin, t) });
        }
    }
    for k in 0..n {
        if parts[k].is_none() {
            let p = f.def.body.parts[k].parent.ok_or("a root without an object")?;
            parts[k] = parts[p].map(|pp| rel(&pp, p, k));
        }
    }
    parts.into_iter().collect::<Option<Vec<_>>>().ok_or_else(|| "a part's pose couldn't be made".into())
}

native!(ragdoll_body => ragdoll_body_impl);
/// `DMS.RagdollBody(model, owner, objects | {pos =, yaw =})`: the fitted body made by the fork in the environment of
/// `owner` (a PhysObj of the model's ragdoll, whose entity the bodies stand in for): NM's capsules and boxes as
/// `dms-homer` hulls them (the fork keeps the hulls), NM's masses, a ragdoll constraint per joint, bound to NM's
/// body (pivots, frames, limits, inertia, materials) and only NM's self-collision pairs colliding (`VBOX3D_NM_006`).
/// Posed as the model's ragdoll is now and moving as its objects do (`objects`: `{{name = bone, obj = PhysObj, pos =
/// its origin}, ...}`, as `DMS.PoseModel` gives them back), or standing as `nm_agent_create` puts one at `pos` (units,
/// its feet's level) facing `yaw` (degrees). Its handle and how long it took (ms).
unsafe fn ragdoll_body_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let owner = unsafe { api.check_physobj(l, 2)? };
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    if nm.version < Version::V006 {
        return Err(format!("{} can't make GMod's ragdolls' bodies: update VPhysics-Box3D ({} needed)", nm.version.name(), Version::V006.name()));
    }
    let body = &f.def.body;
    // `{pos, yaw}` (a table with a `pos`) or the objects' list.
    unsafe { (api.getfield)(l, 3, c"pos".as_ptr()) };
    let stand = unsafe { (api.type_)(l, -1) } > lua::TNIL;
    unsafe { (api.settop)(l, -2) };
    let states: Vec<PartState> = if stand {
        unsafe { (api.getfield)(l, 3, c"pos".as_ptr()) };
        let at = unsafe { api.check_vec3(l, -1) };
        unsafe { (api.settop)(l, -2) };
        unsafe { (api.getfield)(l, 3, c"yaw".as_ptr()) };
        let yaw = unsafe { api.opt_number(l, -1, 0.0) };
        unsafe { (api.settop)(l, -2) };
        body.placed(Vec3::from_array(at?) * M, (yaw? as f32).to_radians())
            .into_iter()
            .map(|(pos, rot)| PartState { pos, rot, vel: Vec3::ZERO, spin: Vec3::ZERO })
            .collect()
    } else {
        parts_from_model(&f, &unsafe { model_objects(api, l, 3)? })?
    };
    if states.iter().any(|s| !(s.pos.is_finite() && s.rot.is_finite() && s.vel.is_finite() && s.spin.is_finite())) {
        return Err("the ragdoll's pose isn't finite".into());
    }
    let started = std::time::Instant::now();
    // SAFETY: a PhysObj Lua handed over.
    let env = unsafe { nm.object_environment(owner) };
    let mut made = NativeBody { objects: vec![], joints: vec![std::ptr::null_mut(); body.parts.len()] };
    let undo = |made: &NativeBody| {
        for &j in &made.joints {
            // SAFETY: made below, not destroyed yet.
            unsafe { nm.destroy_joint(j) };
        }
        for &o in &made.objects {
            unsafe { nm.destroy_body(o) };
        }
    };
    for (p, s) in body.parts.iter().zip(&states) {
        let points: Vec<[f32; 3]> = dms_homer::hull::points(&p.shape).iter().map(|v| v.to_array()).collect();
        // SAFETY: `env` is the owner's live environment; outside the step callback (Lua).
        let o = unsafe { nm.create_body(env, owner, &points, p.mass, &s.pos.to_array(), &s.rot.to_array(), NO_SELF_COLLISIONS) }.unwrap_or(std::ptr::null_mut());
        if o.is_null() {
            undo(&made);
            return Err(format!("{}: CreateBody failed", p.name));
        }
        made.objects.push(o);
    }
    for j in &body.joints {
        // SAFETY: two bodies just made.
        let c = unsafe { nm.create_joint(made.objects[j.parent], made.objects[j.part], j.hinge) }.unwrap_or(std::ptr::null_mut());
        if c.is_null() {
            undo(&made);
            return Err(format!("{}: CreateJoint failed", body.parts[j.part].name));
        }
        made.joints[j.part] = c;
    }
    // NM's pivots, frames and limits in the pose they're in, then their motion and NM's own collision pairs.
    let top = unsafe { (api.gettop)(l) };
    let bound = unsafe { crate::bind::bind_body(api, l, body, &made.objects) };
    unsafe { (api.settop)(l, top) };
    if let Err(e) = bound {
        undo(&made);
        return Err(format!("bind: {e}"));
    }
    for (&o, s) in made.objects.iter().zip(&states) {
        let state = PhysicsNMBodyState { position: s.pos.to_array(), rotation: s.rot.to_array(), velocity: s.vel.to_array(), angular_velocity: s.spin.to_array() };
        // SAFETY: a body just made, outside the step callback.
        unsafe { nm.set_body_state(o, &state) };
    }
    // SAFETY: as above.
    if unsafe { nm.set_body_collision_pairs(&made.objects, &body.pairs) } != Some(true) {
        undo(&made);
        return Err("SetBodyCollisionPairs refused NM's self-collision pairs".into());
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut slot = NATIVE.lock().unwrap_or_else(|e| e.into_inner());
    let (next, map) = slot.get_or_insert_with(|| (0, HashMap::new()));
    *next += 1;
    map.insert(*next, made);
    unsafe {
        (api.pushnumber)(l, f64::from(*next));
        (api.pushnumber)(l, ms);
    }
    Ok(2)
}

native!(ragdoll_warm => ragdoll_warm_impl);
/// `DMS.RagdollWarm(model, owner, part)`: the fork's hull of the fitted body's part `part` (0-based) made ahead of the
/// body (a body made and gone at once; the fork keeps the hull, `CreateBody`), so the first body of a model doesn't
/// make all 21 in one frame (ROADMAP M11: a death). How long it took (ms); nothing for a part past the last.
unsafe fn ragdoll_warm_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let owner = unsafe { api.check_physobj(l, 2)? };
    let k = unsafe { api.check_number(l, 3)? } as usize;
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    if nm.version < Version::V006 {
        return Err(format!("{} can't make GMod's ragdolls' bodies", nm.version.name()));
    }
    let Some(p) = f.def.body.parts.get(k) else { return Ok(0) };
    let started = std::time::Instant::now();
    let points: Vec<[f32; 3]> = dms_homer::hull::points(&p.shape).iter().map(|v| v.to_array()).collect();
    // SAFETY: a PhysObj Lua handed over; outside the step callback (Lua). The body goes before any step.
    unsafe {
        let env = nm.object_environment(owner);
        let s = nm.body_state(owner).ok_or("GetBodyState failed")?;
        if let Some(o) = nm.create_body(env, owner, &points, p.mass, &s.position, &[0.0, 0.0, 0.0, 1.0], NO_SELF_COLLISIONS).filter(|o| !o.is_null()) {
            nm.destroy_body(o);
        }
        (api.pushnumber)(l, started.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(1)
}

native!(ragdoll_follow => ragdoll_follow_impl);
/// `DMS.RagdollFollow(model, handle, yaw, floor)`: NM on a native body (`DMS.Follow` for a fitted one): `yaw` where
/// it faces (rad), `floor` (m) where NM's frame starts. The agent's id; `DMS.Unfollow` it before `DMS.RagdollRemove`.
unsafe fn ragdoll_follow_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let n = native(unsafe { api.check_number(l, 2)? } as u32)?;
    let yaw = unsafe { api.check_number(l, 3)? } as f32;
    let floor = Vec3::from_array(unsafe { api.check_vec3(l, 4)? });
    // SAFETY: Lua unfollows before it removes the native body.
    let id = unsafe { crate::agents::start_body(n.objects, f.def.clone(), yaw, floor, false)? };
    unsafe { (api.pushnumber)(l, f64::from(id)) };
    Ok(1)
}

/// The native body `handle`'s objects (the bench's fitted runs, `dev.rs`).
pub fn native_objects(handle: u32) -> Result<Vec<*mut IPhysicsObject>, String> {
    native(handle).map(|n| n.objects)
}

/// A fitted body's definition (the bench's fitted runs).
pub fn fitted_def(model: &str) -> Result<Arc<BodyDef>, String> {
    fitted(model).map(|f| f.def.clone())
}

native!(ragdoll_report => ragdoll_report_impl);
/// `DMS.RagdollReport(model, handle)`: the native body as the game has it, for `golden body --fit` (as
/// `DMS_HomerReport` has Homer's): a `part <name> <centre x y z> <rotation x y z w> <centre again> <mass> <inertia's
/// diagonal> <damping lin ang> <tensor, 9>` line a part (m, kg, kg m^2; no bone of its own, so its centre stands for
/// the origin), then `joints <found by FindConstraint> <objects>`.
unsafe fn ragdoll_report_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let n = native(unsafe { api.check_number(l, 2)? } as u32)?;
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    let body = &f.def.body;
    let mut out = vec![];
    for (p, &o) in body.parts.iter().zip(&n.objects) {
        // SAFETY: the native body's live objects, on the main thread (Lua).
        let s = unsafe { nm.body_state(o) }.ok_or("GetBodyState failed")?;
        let t = unsafe { nm.body_inertia(o) }.unwrap_or([0.0; 9]);
        let (mass, [lin, ang]) = unsafe { (vphysics_nm::object::mass(o), vphysics_nm::object::damping(o)) };
        let [x, y, z] = s.position;
        let [qx, qy, qz, qw] = s.rotation;
        let tensor: Vec<String> = t.iter().map(|v| format!("{v:.9}")).collect();
        out.push(format!(
            "part {} {x:.9} {y:.9} {z:.9} {qx:.9} {qy:.9} {qz:.9} {qw:.9} {x:.9} {y:.9} {z:.9} {mass:.9} {:.9} {:.9} {:.9} {lin:.9} {ang:.9} {}",
            p.name, t[0], t[4], t[8], tensor.join(" ")
        ));
    }
    // SAFETY: as above.
    let joints = body.joints.iter().filter(|j| !unsafe { nm.find_constraint(n.objects[j.parent], n.objects[j.part]) }.is_null()).count();
    out.push(format!("joints {joints} {}", n.objects.len()));
    unsafe { api.push_str(l, &out.join("\n")) };
    Ok(1)
}

native!(ragdoll_remove => ragdoll_remove_impl);
/// `DMS.RagdollRemove(handle)`: the native body's joints and bodies gone (its agent taken off first, `DMS.Unfollow`).
unsafe fn ragdoll_remove_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let handle = unsafe { api.check_number(l, 1)? } as u32;
    let n = NATIVE.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|(_, m)| m.remove(&handle));
    if let (Some(n), Some(nm)) = (n, vphysics_nm::get()) {
        for &j in &n.joints {
            // SAFETY: made by `ragdoll_body`, not destroyed yet.
            unsafe { nm.destroy_joint(j) };
        }
        for &o in &n.objects {
            unsafe { nm.destroy_body(o) };
        }
    }
    Ok(0)
}

native!(pose_model => pose_model_impl);
/// `DMS.PoseModel(model, handle)`: where the model's mapped bones are with the native body's parts as they are now
/// (S4): `{[bone name] = {pos (units), ang = {x = pitch, y = yaw, z = roll}, rot (quaternion), vel (m/s, the bone's
/// origin's), spin (rad/s, world)}}`, for posing the model every tick and for handing the motion back on detach.
unsafe fn pose_model_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let f = fitted(&unsafe { api.check_string(l, 1)? })?;
    let parts = native(unsafe { api.check_number(l, 2)? } as u32)?.objects;
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    let states = parts
        .iter()
        // SAFETY: the native body's live objects.
        .map(|&o| unsafe { nm.body_state(o) }.ok_or("GetBodyState failed"))
        .collect::<Result<Vec<_>, _>>()?;
    unsafe { (api.createtable)(l, 0, f.posed.len() as c_int) };
    for (name, k, at, turn) in &f.posed {
        let s = states[*k];
        let (p, q) = (Vec3::from_array(s.position), Quat::from_array(s.rotation).normalize());
        let (v, w) = (Vec3::from_array(s.velocity), Vec3::from_array(s.angular_velocity));
        let (pos, rot) = (p + q * *at, (q * *turn).normalize());
        let [pitch, yaw, roll] = angles(rot);
        let name = std::ffi::CString::new(name.as_str()).unwrap_or_default();
        unsafe {
            (api.createtable)(l, 0, 5);
            api.set_vector(l, c"pos", &(pos / M).to_array());
            api.set_vector(l, c"ang", &[pitch, yaw, roll]);
            api.set_vector(l, c"rot", &rot.to_array());
            api.set_vector(l, c"vel", &(v + w.cross(pos - p)).to_array());
            api.set_vector(l, c"spin", &w.to_array());
            (api.setfield)(l, -2, name.as_ptr());
        }
    }
    Ok(1)
}
