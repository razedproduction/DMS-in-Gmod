//! dms-homer: Homer's ragdoll for Garry's Mod, made from the user's copy of GTA IV for NM's body one to one
//! (ROADMAP M3, docs/adr/0013). Never shipped: it's made on the user's machine (docs/adr/0006).

pub mod hull;
pub mod phy;
pub mod smd;
pub mod visual;

use delirium::prelude::*;
use dms_game::body::Body;

/// The model's path under `garrysmod/models/` and its materials' under `garrysmod/materials/`.
pub const MODEL: &str = "dms/homer.mdl";
pub const MATERIALS: &str = "models/dms/homer";
/// The surface Homer's parts are (Source's, for sounds and impacts only: friction is NM's business).
pub const SURFACE: &str = "flesh";

/// The bones: NM's parts, in part order, standing as `nm_agent_create` puts an agent at the origin facing yaw 0
/// (model space, metres: the model's origin is on the floor under the pelvis).
pub fn bones(body: &Body) -> Vec<smd::Bone> {
    body.placed(Vec3::ZERO, 0.0)
        .into_iter()
        .zip(&body.parts)
        .map(|((pos, rot), p)| smd::Bone { name: p.name.to_string(), parent: p.parent, pos, rot })
        .collect()
}

/// Each part's hull (part frame): points and triangles.
pub fn hulls(body: &Body) -> Vec<(Vec<Vec3>, Vec<[usize; 3]>)> {
    body.parts.iter().map(|p| {
        let pts = hull::points(&p.shape);
        let tris = hull::hull(&pts);
        (pts, tris)
    }).collect()
}

/// The hulls as triangles skinned to their bones, in model space.
pub fn hull_triangles(bones: &[smd::Bone], hulls: &[(Vec<Vec3>, Vec<[usize; 3]>)], material: &str) -> Vec<smd::Triangle> {
    let mut out = vec![];
    for (k, (pts, tris)) in hulls.iter().enumerate() {
        let (bp, br) = (bones[k].pos, bones[k].rot);
        for t in tris {
            let [a, b, c] = t.map(|i| bp + br * pts[i]);
            let n = (b - a).cross(c - a).normalize_or_zero();
            let v = |p: Vec3| smd::Vertex { pos: p, normal: n, uv: [0.0, 0.0], weights: vec![(k, 1.0)] };
            out.push(smd::Triangle { material: material.into(), v: [v(a), v(b), v(c)] });
        }
    }
    out
}

/// `.qc` for studiomdl: the reference mesh, the ragdoll's one sequence and its collision joints. `$origin`'s
/// -90 undoes studiomdl's default turn of the root, so model space stays NM's.
pub fn qc(body: &Body, reference: &str, physics: &str) -> String {
    let mass: f32 = body.parts.iter().map(|p| p.mass).sum();
    let mut s = format!(
        "$modelname \"{MODEL}\"\n$origin 0 0 0 -90\n$cdmaterials \"{MATERIALS}/\"\n$surfaceprop \"{SURFACE}\"\n\
         $body homer \"{reference}\"\n$sequence ragdoll \"{reference}\" activity ACT_DIERAGDOLL 1 fps 30\n\
         {}$collisionjoints \"{physics}\" {{\n\t$mass {mass}\n\t$inertia 1\n\t$damping 0\n\t$rotdamping 0\n\t$rootbone \"{}\"\n",
        visual::texture_group(),
        body.parts[0].name
    );
    for &(a, b) in &body.pairs {
        s += &format!("\t$jointcollide \"{}\" \"{}\"\n", body.parts[a].name, body.parts[b].name);
    }
    s += "}\n";
    s
}

/// A ragdoll constraint's axis limits in the child's bone frame (degrees, min max per X Y Z) standing in for NM's
/// joint before `bind.rs` gives the constraint NM's own frame and limits (`SetJointLimits`): NM's range around
/// the standing pose, taken onto the bone's axes; a hinge only about the bone axis nearest NM's.
pub fn constraint_limits(body: &Body, k: usize) -> [[f32; 2]; 3] {
    let Some(j) = body.joints.iter().find(|j| j.part == k) else { return [[0.0; 2]; 3] };
    let (pr, cr) = (body.parts[j.parent].rot, body.parts[k].rot);
    let rest = ((pr * j.frame_parent).inverse() * (cr * j.frame_child)).normalize();
    let rest = if rest.w < 0.0 { -rest } else { rest };
    let w0 = rest.to_scaled_axis();
    let r = Mat3::from_quat(j.frame_child);
    let deg = |x: f32| x.to_degrees().clamp(-180.0, 180.0);
    if j.hinge {
        let z = r.z_axis;
        let a = (0..3).max_by(|&i, &l| z[i].abs().total_cmp(&z[l].abs())).unwrap_or(0);
        let s = z[a].signum();
        let (lo, hi) = (s * (j.limits[0] - w0.z), s * (j.limits[1] - w0.z));
        let mut out = [[0.0; 2]; 3];
        out[a] = [deg(lo.min(hi)), deg(lo.max(hi))];
        return out;
    }
    std::array::from_fn(|i| {
        let row = r.row(i);
        let c = -row.dot(w0);
        let h: f32 = (0..3).map(|l| row[l].abs() * j.limits[l]).sum();
        [deg(c - h), deg(c + h)]
    })
}

/// The `.phy` text for `body`: studiomdl's solids (their order and volumes) with NM's masses, no damping, unit
/// inertia factor; the constraints of [`constraint_limits`]; NM's self-collision pairs only.
pub fn phy_text(body: &Body, compiled: &[phy::Block]) -> Result<String, String> {
    let solids: Vec<&phy::Block> = compiled.iter().filter(|b| b.name == "solid").collect();
    let index_of = |name: &str| -> Result<usize, String> {
        solids.iter().find(|b| b.get("name") == Some(name)).and_then(|b| b.get("index")?.parse().ok())
            .ok_or(format!(".phy: no solid for {name}"))
    };
    if solids.len() != body.parts.len() {
        return Err(format!(".phy: {} solids for {} parts", solids.len(), body.parts.len()));
    }
    // The game takes the solids in the order they come and wants it to be their indices' (`RagdollAddSolid`).
    let mut order: Vec<(usize, &dms_game::body::Part)> =
        body.parts.iter().map(|p| index_of(p.name).map(|i| (i, p))).collect::<Result<_, _>>()?;
    order.sort_by_key(|(i, _)| *i);
    let mut out = vec![];
    for (i, p) in order {
        let volume = solids.iter().find(|b| b.get("name") == Some(p.name)).and_then(|b| b.get("volume")).unwrap_or("0");
        let kv = [
            ("index", i.to_string()), ("name", p.name.into()), ("parent", p.parent.map_or(String::new(), |q| body.parts[q].name.into())), ("mass", format!("{}", p.mass)), ("surfaceprop", SURFACE.into()),
            ("damping", "0".into()), ("rotdamping", "0".into()), ("inertia", "1".into()), ("volume", volume.into()),
        ];
        out.push(phy::Block { name: "solid".into(), keys: kv.into_iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (k.to_string(), v)).collect() });
    }
    for j in &body.joints {
        let l = constraint_limits(body, j.part);
        let mut keys = vec![("parent".to_string(), index_of(body.parts[j.parent].name)?.to_string()),
                            ("child".to_string(), index_of(body.parts[j.part].name)?.to_string())];
        for (a, [lo, hi]) in ["x", "y", "z"].iter().zip(l) {
            keys.push((format!("{a}min"), format!("{lo:.6}")));
            keys.push((format!("{a}max"), format!("{hi:.6}")));
            keys.push((format!("{a}friction"), "0".into()));
        }
        out.push(phy::Block { name: "ragdollconstraint".into(), keys });
    }
    let mut rules = vec![];
    for &(a, b) in &body.pairs {
        rules.push(("collisionpair".into(), format!("{},{}", index_of(body.parts[a].name)?, index_of(body.parts[b].name)?)));
    }
    out.push(phy::Block { name: "collisionrules".into(), keys: rules });
    let mass: f32 = body.parts.iter().map(|p| p.mass).sum();
    out.push(phy::Block {
        name: "editparams".into(),
        keys: vec![("rootname".into(), body.parts[0].name.into()), ("totalmass".into(), format!("{mass}"))],
    });
    Ok(phy::text(&out))
}

/// Each solid's centre of mass on its bone's origin (NM's part centre) and its inertia NM's tensor's diagonal in
/// the part's frame: VPhysics keeps a body's inertia diagonal in its own frame, so the tensor's off-diagonal
/// terms can't be given (docs/adr/0013).
pub fn set_mass_properties(body: &Body, phy: &mut phy::Phy) -> Result<(), String> {
    let blocks = phy::blocks(&phy.text);
    for p in &body.parts {
        let i: usize = blocks.iter().find(|b| b.name == "solid" && b.get("name") == Some(p.name))
            .and_then(|b| b.get("index")?.parse().ok()).ok_or(format!(".phy: no solid {}", p.name))?;
        let t = p.inertia;
        let block = phy.blocks.get_mut(i).ok_or(format!(".phy: no solid {i}"))?;
        phy::set_mass_properties(block, Vec3::new(t.x_axis.x, t.y_axis.y, t.z_axis.z) / p.mass)?;
    }
    phy.solids = phy.blocks.iter().map(|b| phy::solid(b)).collect::<Result<_, _>>()?;
    Ok(())
}

/// How the compiled `.phy` holds up against NM's body, per part: the hull's centre of mass; its faces as the fork
/// grows them ([`hull::INFLATION`]) against NM's surface (how far out each face plane sits from NM's shape's
/// support along its normal: the worst in and out); its points; its volume grown by the inflation (`V + A d`,
/// edges left out) against NM's shape's; the solid's mass against NM's.
pub struct PhyCheck {
    pub name: &'static str,
    pub com: f32,
    pub face_out: f32,
    pub face_in: f32,
    pub points: usize,
    pub volume: f64,
    pub nm_volume: f64,
    pub mass: Option<f32>,
    pub nm_mass: f32,
    /// IVP's unit inertia of the hull (per kg, Source's axes in the bone frame).
    pub unit_inertia: Vec3,
}

/// `phy` (compiled and rewritten) against `body`. The solids' points are in their bones' frames.
pub fn check_phy(body: &Body, phy: &phy::Phy) -> Result<Vec<PhyCheck>, String> {
    let blocks = phy::blocks(&phy.text);
    let solids: Vec<&phy::Block> = blocks.iter().filter(|b| b.name == "solid").collect();
    body.parts.iter().map(|p| {
        let b = solids.iter().find(|b| b.get("name") == Some(p.name)).ok_or(format!(".phy: no solid {}", p.name))?;
        let i: usize = b.get("index").and_then(|i| i.parse().ok()).ok_or(".phy: a solid without an index")?;
        let s = phy.solids.get(i).ok_or(format!(".phy: no solid {i}"))?;
        let centre = s.points.iter().copied().sum::<Vec3>() / s.points.len().max(1) as f32;
        let (mut out, mut inside, mut area) = (f32::MIN, f32::MAX, 0.0f64);
        for t in &s.triangles {
            let [a, b2, c] = t.map(|k| s.points[k]);
            let cr = (b2 - a).cross(c - a);
            if cr.length() < 1e-12 {
                continue;
            }
            area += cr.length() as f64 / 2.0;
            let n = if cr.dot(a - centre) >= 0.0 { cr.normalize() } else { -cr.normalize() };
            let d = n.dot(a) + hull::INFLATION - hull::support(&p.shape, n);
            out = out.max(d);
            inside = inside.min(d);
        }
        Ok(PhyCheck {
            name: p.name,
            com: s.mass_center.length(),
            face_out: out,
            face_in: -inside,
            points: s.points.len(),
            volume: hull::volume(&s.points, &s.triangles).abs() + area * hull::INFLATION as f64,
            nm_volume: hull::shape_volume(&p.shape),
            mass: b.get("mass").and_then(|m| m.parse().ok()),
            nm_mass: p.mass,
            unit_inertia: s.inertia,
        })
    }).collect()
}

/// Distance of `p` (part frame) outside NM's shape (negative inside).
pub fn shape_distance(shape: &dms_game::body::Shape, p: Vec3) -> f32 {
    match *shape {
        dms_game::body::Shape::Capsule { rot, radius, half } => {
            let l = rot.inverse() * p;
            Vec3::new(l.x, l.y - l.y.clamp(-half, half), l.z).length() - radius
        }
        dms_game::body::Shape::Box { rot, half } => {
            let m = dms_game::body::BOX_MARGIN;
            let l = (rot.inverse() * p).abs() - (half - Vec3::splat(m)).max(Vec3::ZERO);
            l.max(Vec3::ZERO).length() + l.max_element().min(0.0) - m
        }
    }
}

/// Garry's Mod in Steam's main library, from the registry (`HKCU\Software\Valve\Steam\SteamPath`).
pub fn steam_gmod() -> Option<std::path::PathBuf> {
    let o = std::process::Command::new("reg").args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"]).output().ok()?;
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    let path = text.lines().find(|l| l.contains("SteamPath"))?.split("REG_SZ").nth(1)?.trim().to_string();
    let g = std::path::PathBuf::from(path).join("steamapps").join("common").join("GarrysMod");
    g.is_dir().then_some(g)
}
