//! `golden body` (ROADMAP M3): `dms-homer`'s port of Homer's body against the Playroom's, step by step.
//!
//! 1. build: the body DMS reads from the game (`dms_game::body`, what `dms-homer` and `bind.rs` use) against the
//!    one of the Playroom's `NM_DUMP`, and both against the SDK's C API on that dump (`nm_build_part_*`,
//!    `nm_agent_get_joint`): 1e-5;
//! 2. the compiled `homer.phy` against that body: solids, masses, centres of mass, hulls, inertia, constraints,
//!    self-collision pairs;
//! 3. in the game (the bench, `dms-ctl`): Homer spawned and bound, his 21 bodies' frames, masses and inertia
//!    against NM's; then dropped without NM: nothing explodes, joints hold together and bend within NM's limits.

use std::path::Path;
use std::time::Duration;

use delirium::prelude::*;
use delirium_physics as sdk;
use dms_game::body::Body;

/// Build tolerance (ROADMAP M3).
const BUILD_TOL: f32 = 1e-5;
/// In the game: positions (m), rotations (rad), inertia (relative, Frobenius).
const POS_TOL: f32 = 0.001;
const ROT_TOL: f32 = 0.5 * std::f32::consts::PI / 180.0;
const INERTIA_TOL: f32 = 0.01;

/// Worst gaps, by what was compared.
#[derive(Default)]
struct Worst(Vec<(String, f32, String)>);

impl Worst {
    fn add(&mut self, what: &str, gap: f32, at: &str) {
        match self.0.iter_mut().find(|w| w.0 == what) {
            Some(w) if gap > w.1 || gap.is_nan() => (w.1, w.2) = (gap, at.into()),
            Some(_) => {}
            None => self.0.push((what.into(), gap, at.into())),
        }
    }

    fn report(&self, tol: f32) -> bool {
        let mut ok = true;
        for (what, gap, at) in &self.0 {
            let pass = *gap <= tol;
            ok &= pass;
            println!("  {:<28} {:>12.3e}  {:<18} {}", what, gap, at, if pass { "ok" } else { "OFF" });
        }
        ok
    }
}

fn qgap(a: Quat, b: Quat) -> f32 {
    // atan2 of the relative turn's parts: acos near 1 is all rounding in f32.
    let d = (a.inverse() * b).normalize();
    2.0 * Vec3::new(d.x, d.y, d.z).length().atan2(d.w.abs())
}

fn vgap(a: Vec3, b: Vec3) -> f32 {
    (a - b).length()
}

fn mgap(a: Mat3, b: Mat3) -> f32 {
    (a - b).to_cols_array().iter().map(|x| x.abs()).fold(0.0, f32::max)
}

/// Two bodies, field by field.
fn compare(a: &Body, b: &Body, w: &mut Worst) -> Result<(), String> {
    if a.parts.len() != b.parts.len() || a.joints.len() != b.joints.len() {
        return Err(format!("{} parts, {} joints against {}, {}", a.parts.len(), a.joints.len(), b.parts.len(), b.joints.len()));
    }
    for (p, q) in a.parts.iter().zip(&b.parts) {
        if p.name != q.name || p.bone != q.bone || p.parent != q.parent {
            return Err(format!("part {} ({}, parent {:?}) against {} ({}, parent {:?})", p.name, p.bone, p.parent, q.name, q.bone, q.parent));
        }
        w.add("part mass (kg)", (p.mass - q.mass).abs(), p.name);
        w.add("part radius, half (m)", (p.radius - q.radius).abs().max((p.half - q.half).abs()), p.name);
        w.add("part frame position (m)", vgap(p.pos, q.pos), p.name);
        w.add("part frame rotation (rad)", qgap(p.rot, q.rot), p.name);
        let shape = match (p.shape, q.shape) {
            (dms_game::body::Shape::Capsule { rot: r1, radius: a1, half: h1 }, dms_game::body::Shape::Capsule { rot: r2, radius: a2, half: h2 }) => {
                qgap(r1, r2).max((a1 - a2).abs()).max((h1 - h2).abs())
            }
            (dms_game::body::Shape::Box { rot: r1, half: e1 }, dms_game::body::Shape::Box { rot: r2, half: e2 }) => qgap(r1, r2).max(vgap(e1, e2)),
            _ => f32::INFINITY,
        };
        w.add("part shape (m, rad)", shape, p.name);
        w.add("part inertia (kg m^2)", mgap(p.inertia, q.inertia), p.name);
    }
    for (j, k) in a.joints.iter().zip(&b.joints) {
        let name = a.parts[j.part].name;
        if j.part != k.part || j.parent != k.parent || j.hinge != k.hinge {
            return Err(format!("joint {name}: {}<-{} {} against {}<-{} {}", j.part, j.parent, j.hinge, k.part, k.parent, k.hinge));
        }
        w.add("joint anchors (m)", vgap(j.anchor_parent, k.anchor_parent).max(vgap(j.anchor_child, k.anchor_child)), name);
        w.add("joint frames (rad)", qgap(j.frame_parent, k.frame_parent).max(qgap(j.frame_child, k.frame_child)), name);
        w.add("joint limits (rad)", (0..3).map(|i| (j.limits[i] - k.limits[i]).abs()).fold(0.0, f32::max), name);
    }
    if a.pairs != b.pairs {
        return Err(format!("self-collision pairs: {} against {}", a.pairs.len(), b.pairs.len()));
    }
    Ok(())
}

/// The body against the SDK's C API on `dump` (part info and frames, an agent's joints at the origin).
fn against_sdk(body: &Body, dump: &str, w: &mut Worst) -> Result<(), String> {
    let c = std::ffi::CString::new(dump).map_err(|e| e.to_string())?;
    // SAFETY: the SDK's own calls on a build and an agent made here, with outputs as big as they write.
    unsafe {
        let b = sdk::nm_build_from_text(c.as_ptr());
        if b.is_null() {
            return Err("nm_build_from_text failed".into());
        }
        let a = sdk::nm_agent_create(b, [0.0f32; 3].as_ptr(), 0.0, 0.3);
        let placed = body.placed(Vec3::ZERO, 0.0);
        let mut r = Ok(());
        for (i, p) in body.parts.iter().enumerate() {
            let mut name = [0u8; 64];
            sdk::nm_part_name(i as i32, name.as_mut_ptr().cast(), 64);
            let name = std::ffi::CStr::from_bytes_until_nul(&name).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let (mut m, mut rad, mut half, mut parent) = (0.0, 0.0, 0.0, -1);
            let (mut pos, mut rot) = ([0.0f32; 3], [0.0f32; 4]);
            sdk::nm_build_part_info(b, i as i32, &mut m, &mut rad, &mut half, &mut parent);
            sdk::nm_build_part_frame(b, i as i32, pos.as_mut_ptr(), rot.as_mut_ptr());
            if name != p.name || parent != p.parent.map_or(-1, |q| q as i32) {
                r = Err(format!("part {i}: the SDK has {name} (parent {parent}), the body {} ({:?})", p.name, p.parent));
                break;
            }
            w.add("sdk: part mass (kg)", (m - p.mass).abs(), p.name);
            w.add("sdk: part radius, half (m)", (rad - p.radius).abs().max((half - p.half).abs()), p.name);
            w.add("sdk: part frame position (m)", vgap(Vec3::from_array(pos), p.pos), p.name);
            w.add("sdk: part frame rotation (rad)", qgap(Quat::from_array(rot), p.rot), p.name);
        }
        let poses: Vec<(Vec3, Quat)> = placed.clone();
        for j in &body.joints {
            let (mut parent, mut pivot, mut frame, mut limits, mut arm) = (-1, [0.0f32; 3], [0.0f32; 4], [0.0f32; 3], 0.0);
            let kind = sdk::nm_agent_get_joint(a, j.part as i32, &mut parent, pivot.as_mut_ptr(), frame.as_mut_ptr(), limits.as_mut_ptr(), &mut arm);
            let name = body.parts[j.part].name;
            if kind != if j.hinge { 2 } else { 1 } || parent != j.parent as i32 {
                r = Err(format!("joint {name}: the SDK has kind {kind}, parent {parent}"));
                break;
            }
            let (piv, frames) = body.joint_world(j, &poses);
            w.add("sdk: joint pivot (m)", vgap(Vec3::from_array(pivot), piv), name);
            w.add("sdk: joint frame (rad)", qgap(Quat::from_array(frame), frames[0]), name);
            let n = if j.hinge { 2 } else { 3 };
            w.add("sdk: joint limits (rad)", (0..n).map(|i| (limits[i] - j.limits[i]).abs()).fold(0.0, f32::max), name);
        }
        sdk::nm_agent_destroy(a);
        sdk::nm_build_destroy(b);
        r
    }
}

/// Step 1: DMS's body (from the game) against the Playroom's dump and the SDK's API.
pub fn build(ours: &Body, ours_dump: &str, theirs_dump: &str) -> Result<bool, String> {
    println!("golden: body 1/3: build (dms-game/dms-homer) against the Playroom's NM_DUMP and the SDK's C API, tolerance {BUILD_TOL:e}");
    let (b, frames) = delirium::ragdoll::Build::undump(theirs_dump);
    let theirs = Body::new(&b, &frames);
    let mut w = Worst::default();
    compare(ours, &theirs, &mut w)?;
    against_sdk(ours, theirs_dump, &mut w)?;
    let same_dump = ours_dump == theirs_dump;
    println!("  dump byte for byte: {}", if same_dump { "yes" } else { "NO" });
    println!("  {} parts, {} joints ({} hinges), {} self-collision pairs", ours.parts.len(), ours.joints.len(),
             ours.joints.iter().filter(|j| j.hinge).count(), ours.pairs.len());
    Ok(w.report(BUILD_TOL) && same_dump)
}

/// Step 2: the compiled `.phy` against the body.
pub fn phy(body: &Body, path: &Path) -> Result<bool, String> {
    println!("golden: body 2/3: {} against the build", path.display());
    let data = std::fs::read(path).map_err(|e| format!("{}: {e} (make the model with dms-homer)", path.display()))?;
    let phy = dms_homer::phy::read(&data)?;
    let checks = dms_homer::check_phy(body, &phy)?;
    let blocks = dms_homer::phy::blocks(&phy.text);
    let mut ok = phy.solids.len() == body.parts.len();
    let mut w = Worst::default();
    for (c, p) in checks.iter().zip(&body.parts) {
        w.add("solid mass - NM's (kg)", c.mass.map_or(f32::INFINITY, |m| (m - c.nm_mass).abs()), c.name);
        w.add("solid centre of mass (m)", c.com, c.name);
        let t = p.inertia;
        let diag = Vec3::new(t.x_axis.x, t.y_axis.y, t.z_axis.z) / p.mass;
        w.add("unit inertia - NM's diagonal", ((c.unit_inertia - diag) / diag).abs().max_element(), c.name);
    }
    ok &= w.report(BUILD_TOL);
    let mut hull = Worst::default();
    for c in &checks {
        hull.add("hull faces outside NM (m)", c.face_out.max(0.0), c.name);
        hull.add("hull faces inside NM (m)", c.face_in.max(0.0), c.name);
    }
    let mut volume = Worst::default();
    for c in &checks {
        volume.add("hull volume gap (fraction)", (c.volume / c.nm_volume - 1.0).abs() as f32, c.name);
    }
    println!("  hulls (VPhysics-Box3D grows them by {:.2} mm; a polygon can't be a capsule: 5 mm, 5 % allowed):", dms_homer::hull::INFLATION * 1000.0);
    ok &= hull.report(0.005) & volume.report(0.05);
    // Constraints: one per joint, a hinge one free axis, a ball more; pairs NM's.
    let index = |name: &str| blocks.iter().find(|b| b.name == "solid" && b.get("name") == Some(name)).and_then(|b| b.get("index")?.parse::<usize>().ok());
    let constraints: Vec<&dms_homer::phy::Block> = blocks.iter().filter(|b| b.name == "ragdollconstraint").collect();
    let mut joints_ok = constraints.len() == body.joints.len();
    for j in &body.joints {
        let (p, c) = (index(body.parts[j.parent].name), index(body.parts[j.part].name));
        let found = constraints.iter().find(|b| b.get("parent").and_then(|v| v.parse().ok()) == p && b.get("child").and_then(|v| v.parse().ok()) == c);
        let free = found.map_or(0, |b| ["x", "y", "z"].iter().filter(|a| b.get(&format!("{a}min")) != b.get(&format!("{a}max"))).count());
        if found.is_none() || (j.hinge && free != 1) || (!j.hinge && free < 2) {
            joints_ok = false;
            println!("  constraint {}: {} free axes for a {}", body.parts[j.part].name, free, if j.hinge { "hinge" } else { "ball" });
        }
    }
    let pairs: Vec<(usize, usize)> = blocks.iter().filter(|b| b.name == "collisionrules").flat_map(|b| b.keys.iter())
        .filter(|(k, _)| k == "collisionpair")
        .filter_map(|(_, v)| v.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<usize>().ok()?, b.trim().parse::<usize>().ok()?))))
        .collect();
    let want: Vec<(usize, usize)> = body.pairs.iter().filter_map(|&(a, b)| Some((index(body.parts[a].name)?, index(body.parts[b].name)?))).collect();
    let same_pairs = pairs.len() == want.len() && want.iter().all(|&(a, b)| pairs.contains(&(a, b)) || pairs.contains(&(b, a)));
    println!("  {} solids, {} constraints (hinges 1 free axis, balls 2+): {}; {} self-collision pairs, NM's: {}", phy.solids.len(),
             constraints.len(), if joints_ok { "ok" } else { "OFF" }, pairs.len(), if same_pairs { "ok" } else { "OFF" });
    Ok(ok && joints_ok && same_pairs)
}

/// One part as the game reports it (`DMS_HomerReport`).
struct GamePart {
    name: String,
    pos: Vec3,
    rot: Quat,
    origin: Vec3,
    mass: f32,
    inertia: Vec3,
    damping: [f32; 2],
}

fn report(text: &str) -> Result<((Vec3, f32), Vec<GamePart>), String> {
    let mut spawn = None;
    let mut parts = vec![];
    for line in text.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        let f = |i: usize| w.get(i).and_then(|x| x.parse::<f32>().ok()).ok_or(format!("bad report line: {line}"));
        match w.first().copied() {
            Some("spawn") => spawn = Some((Vec3::new(f(1)?, f(2)?, f(3)?), f(4)?)),
            Some("part") => parts.push(GamePart {
                name: w.get(1).copied().unwrap_or("").into(),
                pos: Vec3::new(f(2)?, f(3)?, f(4)?),
                rot: Quat::from_xyzw(f(5)?, f(6)?, f(7)?, f(8)?).normalize(),
                origin: Vec3::new(f(9)?, f(10)?, f(11)?),
                mass: f(12)?,
                inertia: Vec3::new(f(13)?, f(14)?, f(15)?),
                damping: [f(16)?, f(17)?],
            }),
            Some("error") => return Err(line.into()),
            _ => {}
        }
    }
    Ok((spawn.ok_or(format!("no spawn line in: {text}"))?, parts))
}

/// Step 3: Homer in the game, bound, against NM's body; then dropped.
pub fn game(body: &Body) -> Result<bool, String> {
    println!("golden: body 3/3: in the game (the bench, dms-ctl): Homer spawned and bound against NM's body");
    let bench = dms_ctl::Bench::load()?;
    let mut ch = dms_ctl::Channel::bench(&bench)?;
    let spawn = r#"
local sp = ents.FindByClass("info_player_start")[1]
local at = sp and sp:GetPos() or Vector(0, 0, 0)
local floor = util.TraceLine({ start = at + Vector(0, 0, 64), endpos = at - Vector(0, 0, 4096) }).HitPos
for _, e in ipairs(ents.FindByModel(DMS_HOMER_MODEL)) do e:Remove() end
local ent, why = DMS_SpawnHomer(floor + Vector(120, 40, 0), 37)
if not ent then return "error " .. tostring(why) end
DMS_GOLDEN_HOMER = ent
local joints = 0
local parts = DMS_HomerParts(ent)
for i, b in ipairs(DMS.BuildParts()) do
	if b.parent >= 0 and DMS.FindJoint(parts[b.parent + 1], parts[i]) then joints = joints + 1 end
end
return DMS_HomerReport(ent) .. "\njoints " .. joints .. " " .. ent:GetPhysicsObjectCount()
"#;
    let text = ch.request("lua", spawn, Duration::from_secs(30))?;
    let ((at, yaw), parts) = report(&text)?;
    let (joints, objects) = text.lines().find_map(|l| l.strip_prefix("joints ")).and_then(|l| {
        let mut w = l.split_whitespace().filter_map(|x| x.parse::<usize>().ok());
        Some((w.next()?, w.next()?))
    }).unwrap_or((0, 0));
    let want = body.placed(at, yaw.to_radians());
    let (mut wpos, mut wrot, mut wmass) = (Worst::default(), Worst::default(), Worst::default());
    let mut inertia = Worst::default();
    let mut diag = Worst::default();
    let mut ok = parts.len() == body.parts.len() && objects == body.parts.len() && joints == body.joints.len();
    for ((p, g), (pos, rot)) in body.parts.iter().zip(&parts).zip(&want) {
        if g.name != p.name {
            return Err(format!("the game's part {} where NM's {} is", g.name, p.name));
        }
        wpos.add("frame position (m)", vgap(g.pos, *pos), p.name);
        wpos.add("centre of mass off the origin (m)", vgap(g.pos, g.origin), p.name);
        wrot.add("frame rotation (rad)", qgap(g.rot, *rot), p.name);
        wmass.add("mass (relative)", ((g.mass - p.mass) / p.mass).abs(), p.name);
        wmass.add("damping", g.damping[0].abs().max(g.damping[1].abs()), p.name);
        // Box3D's tensor: GetInertia's diagonal in the body's frame (the fork keeps no other terms).
        let ours = Mat3::from_diagonal(g.inertia);
        let frob = |m: Mat3| m.to_cols_array().iter().map(|x| x * x).sum::<f32>().sqrt();
        inertia.add("inertia tensor (relative)", frob(ours - p.inertia) / frob(p.inertia), p.name);
        let d = Vec3::new(p.inertia.x_axis.x, p.inertia.y_axis.y, p.inertia.z_axis.z);
        diag.add("inertia diagonal (relative)", ((g.inertia - d) / d).abs().max_element(), p.name);
    }
    println!("  {} physics objects, {} of {} joints found by FindConstraint, every joint bound (DMS.BindHomer)", objects, joints, body.joints.len());
    let pos_ok = wpos.report(POS_TOL) & wrot.report(ROT_TOL) & wmass.report(1e-6);
    let diag_ok = diag.report(INERTIA_TOL);
    let tensor_ok = inertia.report(INERTIA_TOL);
    if !tensor_ok {
        println!("  NM's inertia tensors have off-diagonal terms in the parts' frames; VPhysics-Box3D keeps a body's inertia");
        println!("  diagonal in its own frame (CPhysicsObject::m_Inertia, b3MassData set diagonal): see `golden body` notes");
        for p in &body.parts {
            let t = p.inertia;
            println!("    {:16} NM [{:.5} {:.5} {:.5} | {:.5} {:.5} {:.5}] (diagonal | xy xz yz)", p.name, t.x_axis.x, t.y_axis.y,
                     t.z_axis.z, t.y_axis.x, t.z_axis.x, t.z_axis.y);
        }
    }
    ok &= pos_ok && diag_ok;
    let dropped = drop_test(&mut ch, body)?;
    Ok(ok && tensor_ok && dropped)
}

/// Homer, bound but without NM, left to fall from his standing pose for a few seconds: every body finite and
/// slow at the end, every joint's two pivots together, every joint within NM's limits (with a margin).
fn drop_test(ch: &mut dms_ctl::Channel, body: &Body) -> Result<bool, String> {
    const SECONDS: u64 = 4;
    println!("  dropped without NM for {SECONDS} s:");
    std::thread::sleep(Duration::from_secs(SECONDS));
    let text = ch.request("lua", r#"
local ent = DMS_GOLDEN_HOMER
if not IsValid(ent) then return "error Homer's gone" end
local out = { DMS_HomerReport(ent) }
local fastest = 0
for i = 0, ent:GetPhysicsObjectCount() - 1 do
	fastest = math.max(fastest, ent:GetPhysicsObjectNum(i):GetVelocity():Length() * 0.0254)
end
out[#out + 1] = "speed " .. fastest
ent:Remove()
return table.concat(out, "\n")
"#, Duration::from_secs(30))?;
    let (_, parts) = report(&text)?;
    let speed: f32 = text.lines().find_map(|l| l.strip_prefix("speed ")).and_then(|s| s.trim().parse().ok()).unwrap_or(f32::NAN);
    let poses: Vec<(Vec3, Quat)> = parts.iter().map(|g| (g.pos, g.rot)).collect();
    let finite = poses.iter().all(|(p, q)| p.is_finite() && q.is_finite());
    let mut w = Worst::default();
    let mut over = Worst::default();
    for j in &body.joints {
        let name = body.parts[j.part].name;
        let (pp, pr) = poses[j.parent];
        let (cp, cr) = poses[j.part];
        w.add("joint pivots apart (m)", vgap(pp + pr * j.anchor_parent, cp + cr * j.anchor_child), name);
        let rel = ((pr * j.frame_parent).inverse() * (cr * j.frame_child)).normalize();
        let rel = if rel.w < 0.0 { -rel } else { rel };
        // Twist about the frame's Z and the swing that's left; a hinge only turns about Z.
        let twist = Quat::from_xyzw(0.0, 0.0, rel.z, rel.w).normalize();
        let swing = rel * twist.inverse();
        let t = 2.0 * twist.z.atan2(twist.w);
        let s = swing.to_scaled_axis();
        let beyond = if j.hinge {
            (j.limits[0] - t).max(t - j.limits[1]).max(0.0).max(s.length())
        } else {
            let e = ((s.x / j.limits[0].max(1e-3)).powi(2) + (s.y / j.limits[1].max(1e-3)).powi(2)).sqrt();
            ((e - 1.0) * j.limits[0].min(j.limits[1])).max(t.abs() - j.limits[2]).max(0.0)
        };
        over.add("joint past NM's limits (rad)", beyond, name);
        if beyond > 0.0 {
            println!("    {name:16} {} twist {:+.1} deg, swing x {:+.1} y {:+.1} deg; NM's limits {:.1} {:.1} {:.1} deg: {:.1} deg past",
                if j.hinge { "hinge" } else { "ball " }, t.to_degrees(), s.x.to_degrees(), s.y.to_degrees(), j.limits[0].to_degrees(),
                j.limits[1].to_degrees(), j.limits[2].to_degrees(), beyond.to_degrees());
        }
    }
    println!("    every body finite: {}; fastest body {speed:.3} m/s", if finite { "yes" } else { "NO" });
    let a = w.report(0.02);
    // Box3D's soft limits and NM's own margins: 5 degrees.
    let b = over.report(5.0f32.to_radians());
    Ok(finite && speed < 0.5 && a && b)
}
