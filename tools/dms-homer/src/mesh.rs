//! Homer's skinned mesh out of his fragment on PC: `delirium-data`'s `model` reads the Xbox 360's vertex
//! declarations (big-endian), the PC's are little-endian with D3D9's element types, so the walk over the
//! drawable is the same as `model::meshes` and only the vertex decoding is the PC's.

use delirium_data::resource::{Resource, system_size};

pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Up to four (skeleton bone, weight) per vertex.
    pub skin: Vec<Vec<(usize, f32)>>,
    pub indices: Vec<u32>,
    /// The shader's diffuse texture, if it has one.
    pub texture: Option<String>,
}

fn u32_at(d: &[u8], at: usize) -> Result<u32, String> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or(format!("mesh: read out of range at {at:x}"))
}

fn u16_at(d: &[u8], at: usize) -> Result<u16, String> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes(b.try_into().unwrap())).ok_or(format!("mesh: read out of range at {at:x}"))
}

fn f32_at(d: &[u8], at: usize) -> Result<f32, String> {
    u32_at(d, at).map(f32::from_bits)
}

fn ptr(r: &Resource, p: u32) -> Result<usize, String> {
    let off = (p & 0x0FFF_FFFF) as usize;
    match p >> 28 {
        5 => Ok(off),
        6 => Ok(system_size(r.flags) + off),
        _ => Err(format!("mesh: bad pointer {p:08X}")),
    }
}

/// A `pgObjectArray`: pointer and u16 count, each element a pointer.
fn array(r: &Resource, at: usize) -> Result<Vec<usize>, String> {
    let count = u16_at(&r.data, at + 4)? as usize;
    if count == 0 {
        return Ok(vec![]);
    }
    let list = ptr(r, u32_at(&r.data, at)?)?;
    (0..count).map(|i| ptr(r, u32_at(&r.data, list + i * 4)?)).collect()
}

fn half(h: u16) -> f32 {
    let (sign, exp, mant) = ((h >> 15) as u32, ((h >> 10) & 0x1F) as i32, (h & 0x3FF) as f32);
    let v = match exp {
        0 => mant * 2f32.powi(-24),
        31 => f32::NAN,
        e => (1.0 + mant / 1024.0) * 2f32.powi(e - 15),
    };
    if sign == 1 { -v } else { v }
}

/// Bytes of a D3D9-style element type (`grcFvf` on PC): half2, half4, float, float2, float3, float4, ubyte4,
/// color, dec3n.
fn type_size(t: u32) -> Option<usize> {
    match t {
        0 => Some(2),
        1 => Some(4),
        2 => Some(6),
        3 => Some(8),
        4 => Some(4),
        5 => Some(8),
        6 => Some(12),
        7 => Some(16),
        8 | 9 | 0xA => Some(4),
        _ => None,
    }
}

/// The declaration of a geometry: usage bits, stride, the element types (4 bits each).
pub fn declaration(r: &Resource, geometry: usize) -> Result<(u32, usize, u64), String> {
    let d = &r.data;
    let vb = ptr(r, u32_at(d, geometry + 0x0C)?)?;
    let decl = ptr(r, u32_at(d, vb + 0x10)?)?;
    let types = d.get(decl + 8..decl + 16).ok_or("mesh: truncated declaration")?;
    Ok((u32_at(d, decl)?, d[decl + 4] as usize, u64::from_le_bytes(types.try_into().unwrap())))
}

/// The drawable's geometries and their shader index (`model::geometries`).
fn geometries(r: &Resource, drawable: usize) -> Result<Vec<(usize, usize)>, String> {
    let d = &r.data;
    let lod = u32_at(d, drawable + 0x40)?;
    if lod == 0 {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for model in array(r, ptr(r, lod)?)? {
        let shaders = ptr(r, u32_at(d, model + 0x10)?)?;
        for (i, g) in array(r, model + 4)?.into_iter().enumerate() {
            out.push((u16_at(d, shaders + i * 2)? as usize, g));
        }
    }
    Ok(out)
}

const POSITION: u32 = 0;
const BLEND_WEIGHT: u32 = 1;
const BLEND_INDEX: u32 = 2;
const NORMAL: u32 = 3;
const TEXCOORD0: u32 = 6;

/// Every mesh of the drawable (the fragment's root drawable for a ped).
pub fn read(r: &Resource, drawable: usize) -> Result<Vec<Mesh>, String> {
    let d = &r.data;
    let textures = delirium_data::model::shader_params(r, drawable).unwrap_or_default();
    geometries(r, drawable)?
        .into_iter()
        .map(|(shader, g)| {
            let (usage, stride, types) = declaration(r, g)?;
            let vb = ptr(r, u32_at(d, g + 0x0C)?)?;
            let ib = ptr(r, u32_at(d, g + 0x1C)?)?;
            let count = u16_at(d, vb + 4)? as usize;
            let (vdata, idata) = (ptr(r, u32_at(d, vb + 8)?)?, ptr(r, u32_at(d, ib + 8)?)?);
            let map_count = u16_at(d, g + 0x3E)? as usize;
            let map: Vec<u16> = if map_count == 0 { vec![] } else {
                let at = ptr(r, u32_at(d, g + 0x38)?)?;
                (0..map_count).map(|i| u16_at(d, at + i * 2)).collect::<Result<_, _>>()?
            };
            let indices = (0..u32_at(d, ib + 4)? as usize).map(|i| u16_at(d, idata + i * 2).map(u32::from)).collect::<Result<Vec<_>, _>>()?;
            let mut m = Mesh { positions: vec![], normals: vec![], uvs: vec![], skin: vec![], indices, texture: None };
            let (mut weights, mut joints) = (vec![], vec![]);
            let mut offset = 0;
            for bit in 0..16 {
                if usage & (1 << bit) == 0 {
                    continue;
                }
                let t = ((types >> (bit * 4)) & 0xF) as u32;
                let size = type_size(t).ok_or(format!("mesh: element type {t} (usage bit {bit})"))?;
                for v in 0..count {
                    let at = vdata + v * stride + offset;
                    let f = |k: usize| f32_at(d, at + k * 4);
                    let b = |k: usize| d.get(at + k).copied().unwrap_or(0);
                    match (bit, t) {
                        (POSITION, 6) => m.positions.push([f(0)?, f(1)?, f(2)?]),
                        (NORMAL, 6) => m.normals.push([f(0)?, f(1)?, f(2)?]),
                        (BLEND_WEIGHT, 8 | 9) => weights.push([b(0), b(1), b(2), b(3)]),
                        (BLEND_INDEX, 8 | 9) => joints.push([b(0), b(1), b(2), b(3)]),
                        (TEXCOORD0, 1) => m.uvs.push([half(u16_at(d, at)?), half(u16_at(d, at + 2)?)]),
                        (TEXCOORD0, 5) => m.uvs.push([f(0)?, f(1)?]),
                        _ => break,
                    }
                }
                offset += size;
            }
            if offset != stride {
                return Err(format!("mesh: declaration's elements make {offset} bytes, the stride is {stride} (usage {usage:x}, types {types:016x})"));
            }
            m.skin = weights.iter().zip(&joints).map(|(w, j)| {
                (0..4).filter(|&k| w[k] > 0).map(|k| (map.get(j[k] as usize).copied().unwrap_or(j[k] as u16) as usize, w[k] as f32 / 255.0)).collect()
            }).collect();
            m.texture = textures.get(shader).and_then(|p| p.textures.first()).map(|t| t.1.clone());
            Ok(m)
        })
        .collect()
}
