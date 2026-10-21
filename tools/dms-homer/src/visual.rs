//! Homer's look, as the Playroom draws him (`delirium-room`'s `part_mesh`, `part_color`, `PED_COLORS`): each NM
//! part's own capsule (or box) in its capsule frame, red on the left, blue on the right, grey in the middle, see
//! through. The Playroom's other colour schemes are the model's skins.

use delirium::prelude::*;
use dms_game::body::{Body, Shape};

use crate::smd;

/// Segments around a capsule and rings per hemisphere (smooth, unlike the physics hulls).
const AROUND: usize = 24;
const RINGS: usize = 8;

/// A material: its name and `VertexLitGeneric`'s colour (sRGB) and alpha.
pub struct Material {
    pub name: &'static str,
    pub color: [f32; 3],
    pub alpha: f32,
}

const RED: [f32; 3] = [0.85, 0.3, 0.25];
const BLUE: [f32; 3] = [0.3, 0.45, 0.9];
const GREY: [f32; 3] = [0.72, 0.72, 0.74];

/// The materials of the default scheme ("mixed"), by side: what the model's triangles use.
pub const BASE: [Material; 3] = [
    Material { name: "homer_left", color: RED, alpha: 0.55 },
    Material { name: "homer_right", color: BLUE, alpha: 0.55 },
    Material { name: "homer_middle", color: GREY, alpha: 0.6 },
];

/// The Playroom's `PED_COLORS` after "mixed", as skins: each scheme's materials for left, right, middle.
pub const SKINS: [(&str, [Material; 3]); 4] = [
    ("all blue", [Material { name: "homer_blue", color: BLUE, alpha: 0.55 }, Material { name: "homer_blue", color: BLUE, alpha: 0.55 },
                  Material { name: "homer_blue", color: BLUE, alpha: 0.55 }]),
    ("all red", [Material { name: "homer_red", color: RED, alpha: 0.55 }, Material { name: "homer_red", color: RED, alpha: 0.55 },
                 Material { name: "homer_red", color: RED, alpha: 0.55 }]),
    ("solid blue", [Material { name: "homer_solid", color: BLUE, alpha: 1.0 }, Material { name: "homer_solid", color: BLUE, alpha: 1.0 },
                    Material { name: "homer_solid", color: BLUE, alpha: 1.0 }]),
    ("mixed, opaque", [Material { name: "homer_left_opaque", color: RED, alpha: 1.0 },
                       Material { name: "homer_right_opaque", color: BLUE, alpha: 1.0 },
                       Material { name: "homer_middle_opaque", color: GREY, alpha: 1.0 }]),
];

/// `part_color`'s side of a part: 0 left, 1 right, 2 middle.
fn side(name: &str) -> usize {
    if name.ends_with("Left") { 0 } else if name.ends_with("Right") { 1 } else { 2 }
}

/// The material's `.vmt`.
pub fn vmt(m: &Material) -> String {
    let [r, g, b] = m.color;
    let alpha = if m.alpha < 1.0 { format!("\t\"$translucent\" \"1\"\n\t\"$alpha\" \"{}\"\n", m.alpha) } else { String::new() };
    format!("\"VertexLitGeneric\"\n{{\n\t\"$basetexture\" \"models/debug/debugwhite\"\n\t\"$color2\" \"[{r} {g} {b}]\"\n{alpha}}}\n")
}

/// `$texturegroup` for the `.qc`: the default scheme's row, then one row per other scheme.
pub fn texture_group() -> String {
    let row = |ms: &[Material; 3]| ms.iter().map(|m| format!("\"{}\"", m.name)).collect::<Vec<_>>().join(" ");
    let mut s = format!("$texturegroup skinfamilies\n{{\n\t{{ {} }}\n", row(&BASE));
    for (_, ms) in &SKINS {
        s += &format!("\t{{ {} }}\n", row(ms));
    }
    s + "}\n"
}

/// A shape's surface: (position, normal) triangles in the part's frame, counter-clockwise from outside.
fn surface(shape: &Shape) -> Vec<[(Vec3, Vec3); 3]> {
    let mut out = vec![];
    match *shape {
        Shape::Capsule { rot, radius, half } => {
            // Rings from the top pole to the bottom one; the cylinder between the two equators.
            let mut rings: Vec<(f32, f32, f32)> = vec![]; // (y of the centre, sin, cos) of the normal's latitude
            for i in 0..=RINGS {
                let lat = std::f32::consts::FRAC_PI_2 * (1.0 - i as f32 / RINGS as f32);
                rings.push((half, lat.sin(), lat.cos()));
            }
            for i in 0..=RINGS {
                let lat = -std::f32::consts::FRAC_PI_2 * (i as f32 / RINGS as f32);
                rings.push((-half, lat.sin(), lat.cos()));
            }
            let point = |r: (f32, f32, f32), k: usize| {
                let a = k as f32 / AROUND as f32 * std::f32::consts::TAU;
                let n = Vec3::new(r.2 * a.cos(), r.1, r.2 * a.sin());
                (rot * (Vec3::new(0.0, r.0, 0.0) + n * radius), rot * n)
            };
            for w in rings.windows(2) {
                for k in 0..AROUND {
                    let (a, b, c, d) = (point(w[0], k), point(w[0], k + 1), point(w[1], k), point(w[1], k + 1));
                    out.push([a, b, d]);
                    out.push([a, d, c]);
                }
            }
        }
        Shape::Box { rot, half } => {
            for axis in 0..3 {
                for sign in [-1.0f32, 1.0] {
                    let mut n = Vec3::ZERO;
                    n[axis] = sign;
                    let (e1, e2) = [(Vec3::Y, Vec3::Z), (Vec3::Z, Vec3::X), (Vec3::X, Vec3::Y)][axis];
                    let c = |s: f32, t: f32| (rot * ((n + e1 * s + e2 * t) * half), rot * n);
                    let q = [c(-1.0, -1.0), c(1.0, -1.0), c(1.0, 1.0), c(-1.0, 1.0)];
                    // e1 x e2 = n for the positive side; the negative side turns the other way.
                    if sign > 0.0 {
                        out.push([q[0], q[1], q[2]]);
                        out.push([q[0], q[2], q[3]]);
                    } else {
                        out.push([q[0], q[2], q[1]]);
                        out.push([q[0], q[3], q[2]]);
                    }
                }
            }
        }
    }
    out
}

/// Every part's shape on its bone (model space; `parts` the parts' frames), in its side's material.
pub fn triangles(body: &Body, parts: &[(Vec3, Quat)]) -> Vec<smd::Triangle> {
    let mut out = vec![];
    for (k, p) in body.parts.iter().enumerate() {
        let (bp, br) = parts[k];
        let material = BASE[side(p.name)].name;
        for tri in surface(&p.shape) {
            let v = tri.map(|(pos, n)| smd::Vertex { pos: bp + br * pos, normal: br * n, uv: [0.0, 0.0], weights: vec![(k, 1.0)] });
            out.push(smd::Triangle { material: material.into(), v });
        }
    }
    out
}
