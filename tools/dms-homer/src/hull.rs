//! Convex hulls of NM's part shapes, as point clouds and their triangles (incremental hull), made for
//! VPhysics-Box3D: it grows every hull's faces by [`INFLATION`] when it builds the body (as IVP kept its collision
//! margin), welds a `.phy`'s points closer than 1 cm and takes at most 128 points a hull, so the points here are
//! the shape shrunk by the inflation, a centimetre or more apart, under a hundred.

use delirium::prelude::*;
use dms_game::body::Shape;

/// VPhysics-Box3D's `SHAPE_INFLATION` (`conversion.h`: `COLLISION_TOLERANCE` 0.25 - 0.0001 inch), in metres:
/// how far its bodies' faces sit outside the `.phy`'s.
pub const INFLATION: f32 = (0.25 - 0.0001) * 0.0254;
/// Segments around a capsule's equator at most, and its rings per hemisphere below the pole.
pub const AROUND: usize = 20;
pub const RINGS: usize = 4;
/// The shortest edge between a hull's points that the fork's 1 cm welding leaves alone.
const EDGE: f32 = 0.014;

/// The points of `shape`'s hull before the fork grows it (part frame).
pub fn points(shape: &Shape) -> Vec<Vec3> {
    match *shape {
        Shape::Capsule { rot, radius, half } => {
            // Faces grown by INFLATION end up with the apothem whose polygon has the circle's area.
            let around = ((std::f32::consts::TAU * (radius - INFLATION) / EDGE) as usize).clamp(10, AROUND) & !3;
            let rings = if radius - INFLATION >= 0.06 { RINGS } else { RINGS - 1 };
            let n = around as f32;
            let apothem = radius * (std::f32::consts::PI / (n * (std::f32::consts::PI / n).tan())).sqrt();
            let rho = (apothem - INFLATION) / (std::f32::consts::PI / n).cos();
            let mut p = vec![];
            for side in [1.0f32, -1.0] {
                p.push(rot * Vec3::new(0.0, side * (half + radius - INFLATION), 0.0));
                for ring in 0..rings {
                    let lat = ring as f32 / rings as f32 * std::f32::consts::FRAC_PI_2;
                    let (s, c) = lat.sin_cos();
                    let count = ((n * c).round() as usize).max(4);
                    let twist = if ring % 2 == 1 { 0.5 } else { 0.0 };
                    for k in 0..count {
                        let a = (k as f32 + twist) / count as f32 * std::f32::consts::TAU;
                        let v = Vec3::new(c * a.cos() * rho, side * (half + s * rho), c * a.sin() * rho);
                        p.push(rot * v);
                    }
                }
            }
            p
        }
        // NM's 1 cm rounding left out: the fork's welding would take it out anyway.
        Shape::Box { rot, half } => {
            let h = (half - Vec3::splat(INFLATION)).max(Vec3::splat(0.001));
            let mut p = vec![];
            for sx in [-1.0f32, 1.0] {
                for sy in [-1.0f32, 1.0] {
                    for sz in [-1.0f32, 1.0] {
                        p.push(rot * (Vec3::new(sx, sy, sz) * h));
                    }
                }
            }
            p
        }
    }
}

/// How far NM's shape reaches along `n` (part frame, unit): its support.
pub fn support(shape: &Shape, n: Vec3) -> f32 {
    match *shape {
        Shape::Capsule { rot, radius, half } => (rot * Vec3::Y).dot(n).abs() * half + radius,
        Shape::Box { rot, half } => {
            let m = dms_game::body::BOX_MARGIN;
            let inner = (half - Vec3::splat(m)).max(Vec3::ZERO);
            let l = rot.inverse() * n;
            l.x.abs() * inner.x + l.y.abs() * inner.y + l.z.abs() * inner.z + m
        }
    }
}

/// The convex hull of `points`: triangles (indices into `points`), counter-clockwise seen from outside.
pub fn hull(points: &[Vec3]) -> Vec<[usize; 3]> {
    let p: Vec<DVec> = points.iter().map(|v| DVec([v.x as f64, v.y as f64, v.z as f64])).collect();
    let scale = p.iter().map(|v| v.len()).fold(0.0, f64::max).max(1e-9);
    let eps = scale * 1e-9;
    // A starting tetrahedron.
    let a = 0;
    let b = (0..p.len()).max_by(|&i, &j| p[i].sub(p[a]).len().total_cmp(&p[j].sub(p[a]).len())).unwrap_or(0);
    let c = (0..p.len()).max_by(|&i, &j| line_dist(p[a], p[b], p[i]).total_cmp(&line_dist(p[a], p[b], p[j]))).unwrap_or(0);
    let n = p[b].sub(p[a]).cross(p[c].sub(p[a]));
    let d = (0..p.len()).max_by(|&i, &j| n.dot(p[i].sub(p[a])).abs().total_cmp(&n.dot(p[j].sub(p[a])).abs())).unwrap_or(0);
    if n.len() < eps || n.dot(p[d].sub(p[a])).abs() < eps * scale {
        return vec![];
    }
    let mut faces: Vec<[usize; 3]> = if n.dot(p[d].sub(p[a])) > 0.0 {
        vec![[a, c, b], [a, b, d], [b, c, d], [c, a, d]]
    } else {
        vec![[a, b, c], [a, d, b], [b, d, c], [c, d, a]]
    };
    let above = |f: &[usize; 3], q: DVec| {
        let n = p[f[1]].sub(p[f[0]]).cross(p[f[2]].sub(p[f[0]]));
        n.dot(q.sub(p[f[0]])) > eps * n.len().max(1e-30)
    };
    for i in 0..p.len() {
        if [a, b, c, d].contains(&i) {
            continue;
        }
        let seen: Vec<bool> = faces.iter().map(|f| above(f, p[i])).collect();
        if !seen.iter().any(|&s| s) {
            continue;
        }
        // The horizon: edges of seen faces whose other side isn't seen.
        let mut edges = vec![];
        for (f, _) in faces.iter().zip(&seen).filter(|(_, s)| **s) {
            for e in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
                edges.push(e);
            }
        }
        let horizon: Vec<(usize, usize)> = edges.iter().copied().filter(|&(u, v)| !edges.contains(&(v, u))).collect();
        faces = faces.into_iter().zip(seen).filter(|(_, s)| !s).map(|(f, _)| f).collect();
        faces.extend(horizon.into_iter().map(|(u, v)| [u, v, i]));
    }
    faces
}

/// Volume of a closed triangle mesh (outward winding).
pub fn volume(points: &[Vec3], tris: &[[usize; 3]]) -> f64 {
    tris.iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| DVec([points[i].x as f64, points[i].y as f64, points[i].z as f64]));
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

/// NM's shape's own volume.
pub fn shape_volume(shape: &Shape) -> f64 {
    match *shape {
        Shape::Capsule { radius, half, .. } => {
            let (r, h) = (radius as f64, half as f64);
            std::f64::consts::PI * r * r * (2.0 * h + 4.0 / 3.0 * r)
        }
        Shape::Box { half, .. } => {
            let m = dms_game::body::BOX_MARGIN as f64;
            let i = (half - Vec3::splat(dms_game::body::BOX_MARGIN)).max(Vec3::ZERO).as_dvec3();
            // A box swept by a sphere: the box, its faces' slabs, its edges' quarter cylinders, its corners.
            8.0 * i.x * i.y * i.z
                + 8.0 * m * (i.x * i.y + i.y * i.z + i.z * i.x)
                + 2.0 * std::f64::consts::PI * m * m * (i.x + i.y + i.z)
                + 4.0 / 3.0 * std::f64::consts::PI * m * m * m
        }
    }
}

#[derive(Clone, Copy)]
struct DVec([f64; 3]);

impl DVec {
    fn sub(self, o: DVec) -> DVec {
        DVec([self.0[0] - o.0[0], self.0[1] - o.0[1], self.0[2] - o.0[2]])
    }
    fn dot(self, o: DVec) -> f64 {
        self.0[0] * o.0[0] + self.0[1] * o.0[1] + self.0[2] * o.0[2]
    }
    fn cross(self, o: DVec) -> DVec {
        let (a, b) = (self.0, o.0);
        DVec([a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]])
    }
    fn len(self) -> f64 {
        self.dot(self).sqrt()
    }
}

fn line_dist(a: DVec, b: DVec, q: DVec) -> f64 {
    b.sub(a).cross(q.sub(a)).len()
}
