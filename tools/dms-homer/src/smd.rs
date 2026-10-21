//! Valve's SMD (studiomdl data): a skeleton in its reference pose and skinned triangles, in inches.

use std::fmt::Write;

use delirium::prelude::*;

/// Inches per metre (1 Source unit = 0.0254 m).
pub const UNITS_PER_M: f32 = 1.0 / 0.0254;

pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    /// Model space, metres.
    pub pos: Vec3,
    pub rot: Quat,
}

/// One vertex: model space position (metres), normal, uv and its bones' weights.
#[derive(Clone)]
pub struct Vertex {
    pub pos: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
    pub weights: Vec<(usize, f32)>,
}

pub struct Triangle {
    pub material: String,
    /// Counter-clockwise seen from the front.
    pub v: [Vertex; 3],
}

fn num(x: f32) -> String {
    let s = format!("{x:.6}");
    if s == "-0.000000" { "0.000000".into() } else { s }
}

/// Source's `AngleMatrix` of a `RadianEuler` (x, y, z) is `Rz(z) Ry(y) Rx(x)`.
fn euler(q: Quat) -> [f32; 3] {
    let (z, y, x) = q.to_euler(EulerRot::ZYX);
    [x, y, z]
}

pub fn write(bones: &[Bone], triangles: &[Triangle]) -> String {
    let mut s = String::from("version 1\nnodes\n");
    for (i, b) in bones.iter().enumerate() {
        let _ = writeln!(s, "{i} \"{}\" {}", b.name, b.parent.map_or(-1, |p| p as i32));
    }
    s += "end\nskeleton\ntime 0\n";
    for (i, b) in bones.iter().enumerate() {
        let (pos, rot) = match b.parent {
            Some(p) => {
                let inv = bones[p].rot.inverse();
                (inv * (b.pos - bones[p].pos), (inv * b.rot).normalize())
            }
            None => (b.pos, b.rot),
        };
        let p = pos * UNITS_PER_M;
        let e = euler(rot);
        let _ = writeln!(s, "{i} {} {} {} {} {} {}", num(p.x), num(p.y), num(p.z), num(e[0]), num(e[1]), num(e[2]));
    }
    s += "end\n";
    if !triangles.is_empty() {
        s += "triangles\n";
        for t in triangles {
            let _ = writeln!(s, "{}", t.material);
            for v in &t.v {
                let p = v.pos * UNITS_PER_M;
                let first = v.weights.first().map_or(0, |w| w.0);
                let _ = write!(s, "{first} {} {} {} {} {} {} {} {} {}", num(p.x), num(p.y), num(p.z), num(v.normal.x), num(v.normal.y),
                               num(v.normal.z), num(v.uv[0]), num(v.uv[1]), v.weights.len());
                for (b, w) in &v.weights {
                    let _ = write!(s, " {b} {}", num(*w));
                }
                s += "\n";
            }
        }
        s += "end\n";
    }
    s
}
