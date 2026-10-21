//! A compiled `.phy`: its convex solids (IVP compact surfaces) and its key values text, read and rewritten.
//!
//! Layout: `phyheader_t` (size 16, id, solid count, checksum), then per solid an int size and that many bytes
//! (`VPHY` header, then `IVP_Compact_Surface`), then the text. IVP's axes are Source's `(x, -z, y)` in metres.

use delirium::prelude::*;

/// One solid: its hull's points (bone space, metres, Source's axes), triangles, mass centre and IVP's inertia.
pub struct Solid {
    pub points: Vec<Vec3>,
    pub triangles: Vec<[usize; 3]>,
    pub mass_center: Vec3,
    /// IVP's unit rotation inertia (per kg), Source's axes.
    pub inertia: Vec3,
}

pub struct Phy {
    pub head: Vec<u8>,
    pub blocks: Vec<Vec<u8>>,
    pub solids: Vec<Solid>,
    pub text: String,
}

fn i32_at(d: &[u8], at: usize) -> Result<i32, String> {
    d.get(at..at + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).ok_or(format!(".phy: truncated at {at}"))
}

fn f32_at(d: &[u8], at: usize) -> Result<f32, String> {
    i32_at(d, at).map(|v| f32::from_bits(v as u32))
}

/// IVP's point (metres, IVP axes) in Source's axes.
fn source(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, z, -y)
}

pub fn read(d: &[u8]) -> Result<Phy, String> {
    let size = i32_at(d, 0)? as usize;
    let count = i32_at(d, 8)? as usize;
    let mut at = size;
    let (mut blocks, mut solids) = (vec![], vec![]);
    for _ in 0..count {
        let n = i32_at(d, at)? as usize;
        let block = d.get(at + 4..at + 4 + n).ok_or(".phy: truncated solid")?;
        solids.push(solid(block)?);
        blocks.push(block.to_vec());
        at += 4 + n;
    }
    let text = String::from_utf8_lossy(&d[at..]).trim_end_matches('\0').to_string();
    Ok(Phy { head: d[..size].to_vec(), blocks, solids, text })
}

pub fn solid(b: &[u8]) -> Result<Solid, String> {
    // The `VPHY` header (id, version, model type, surface size, drag areas, axis map size) or IVP's surface alone.
    let s = if b.get(0..4) == Some(b"VPHY") { 28 } else { 0 };
    let f = |k: usize| f32_at(b, s + k * 4);
    let mass_center = source(f(0)?, f(1)?, f(2)?);
    let inertia = Vec3::new(f(3)?, f(5)?, f(4)?);
    let root = s + i32_at(b, s + 32)? as usize;
    let (mut points, mut triangles) = (vec![], vec![]);
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let right = i32_at(b, node)?;
        if right == 0 {
            let ledge = node + i32_at(b, node + 4)? as usize;
            ledge_into(b, ledge, &mut points, &mut triangles)?;
        } else {
            stack.push(node + 28);
            stack.push(node + right as usize);
        }
    }
    Ok(Solid { points, triangles, mass_center, inertia })
}

/// An `IVP_Compact_Ledge`'s triangles and points.
fn ledge_into(b: &[u8], ledge: usize, points: &mut Vec<Vec3>, triangles: &mut Vec<[usize; 3]>) -> Result<(), String> {
    let point_at = (ledge as i64 + i32_at(b, ledge)? as i64) as usize;
    let n = u16::from_le_bytes(b.get(ledge + 12..ledge + 14).ok_or(".phy: truncated ledge")?.try_into().unwrap()) as usize;
    let base = points.len();
    let mut most = 0;
    for t in 0..n {
        let tri = ledge + 16 + t * 16;
        let e = |k: usize| i32_at(b, tri + 4 + k * 4).map(|v| (v as u32 & 0xFFFF) as usize);
        let v = [e(0)?, e(1)?, e(2)?];
        most = most.max(v[0]).max(v[1]).max(v[2]);
        triangles.push(v.map(|i| base + i));
    }
    for i in 0..=most {
        let p = point_at + i * 16;
        points.push(source(f32_at(b, p)?, f32_at(b, p + 4)?, f32_at(b, p + 8)?));
    }
    Ok(())
}

/// Sets a solid's mass centre (metres, Source's axes in the bone frame) and its unit rotation inertia (per kg,
/// same axes): `IVP_Compact_Surface`'s first two vectors, which VPhysics takes the body's mass properties from.
pub fn set_mass_properties(block: &mut [u8], center: Vec3, unit_inertia: Vec3) -> Result<(), String> {
    let s = if block.get(0..4) == Some(b"VPHY") { 28 } else { 0 };
    // IVP's axes: Source's (x, y, z) are IVP's (x, -z, y) (`source` the other way); inertia has no sign.
    let values = [center.x, -center.z, center.y, unit_inertia.x, unit_inertia.z, unit_inertia.y];
    let dst = block.get_mut(s..s + 24).ok_or(".phy: truncated solid")?;
    for (chunk, v) in dst.chunks_mut(4).zip(values) {
        chunk.copy_from_slice(&v.to_le_bytes());
    }
    Ok(())
}

/// The file again, with `text` in place of its text.
pub fn write(phy: &Phy, text: &str) -> Vec<u8> {
    let mut out = phy.head.clone();
    for b in &phy.blocks {
        out.extend((b.len() as i32).to_le_bytes());
        out.extend(b);
    }
    out.extend(text.as_bytes());
    out.push(0);
    out
}

/// A key values block of the text: its name and its keys in order.
#[derive(Clone, Debug)]
pub struct Block {
    pub name: String,
    pub keys: Vec<(String, String)>,
}

impl Block {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.keys.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// The text's blocks (`solid {..}`, `ragdollconstraint {..}`, `collisionrules {..}`, `editparams {..}` ...).
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = vec![];
    let mut tokens = vec![];
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => tokens.push(chars.by_ref().take_while(|&c| c != '"').collect::<String>()),
            '{' | '}' => tokens.push(c.to_string()),
            c if c.is_whitespace() => {}
            c => {
                let mut w = c.to_string();
                while let Some(&n) = chars.peek().filter(|n| !n.is_whitespace() && **n != '{' && **n != '}' && **n != '"') {
                    w.push(n);
                    chars.next();
                }
                tokens.push(w);
            }
        }
    }
    let mut it = tokens.into_iter();
    while let Some(name) = it.next() {
        if it.next().as_deref() != Some("{") {
            break;
        }
        let mut keys = vec![];
        loop {
            match it.next() {
                Some(k) if k == "}" => break,
                Some(k) => keys.push((k, it.next().unwrap_or_default())),
                None => break,
            }
        }
        out.push(Block { name, keys });
    }
    out
}

pub fn text(blocks: &[Block]) -> String {
    let mut s = String::new();
    for b in blocks {
        s += &b.name;
        s += " {\n";
        for (k, v) in &b.keys {
            s += &format!("\"{k}\" \"{v}\"\n");
        }
        s += "}\n";
    }
    s
}
