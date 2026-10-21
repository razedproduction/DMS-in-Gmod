//! Homer's textures: a PC texture dictionary's (`.wtd`) DXT images, written as Valve's VTF (7.2).

use delirium_data::resource::{Resource, system_size};

pub struct Texture {
    /// Its name without `pack:/` and `.dds` (`head_diff_000_a_whi`).
    pub name: String,
    pub width: usize,
    pub height: usize,
    /// `DXT1`, `DXT3` or `DXT5`.
    pub fourcc: [u8; 4],
    pub levels: usize,
    /// The mip levels, largest first, back to back.
    pub data: Vec<u8>,
}

fn u32_at(d: &[u8], at: usize) -> Result<u32, String> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or(format!("wtd: read out of range at {at:x}"))
}

fn u16_at(d: &[u8], at: usize) -> Result<u16, String> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes(b.try_into().unwrap())).ok_or(format!("wtd: read out of range at {at:x}"))
}

fn ptr(r: &Resource, p: u32) -> Result<usize, String> {
    let off = (p & 0x0FFF_FFFF) as usize;
    match p >> 28 {
        5 => Ok(off),
        6 => Ok(system_size(r.flags) + off),
        _ => Err(format!("wtd: bad pointer {p:08X}")),
    }
}

fn block_bytes(fourcc: [u8; 4]) -> usize {
    if &fourcc == b"DXT1" { 8 } else { 16 }
}

fn level_size(w: usize, h: usize, fourcc: [u8; 4]) -> usize {
    w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * block_bytes(fourcc)
}

/// Every texture of a PC texture dictionary (`grcTexturePC`: name at 0x14, size at 0x1C, D3D format at 0x20,
/// mip levels at 0x27, pixels at 0x48).
pub fn read(r: &Resource) -> Result<Vec<Texture>, String> {
    let d = &r.data;
    delirium_data::model::dictionary(r)?
        .into_iter()
        .map(|(_, at)| {
            let name_at = ptr(r, u32_at(d, at + 0x14)?)?;
            let name = String::from_utf8_lossy(d[name_at..].split(|&c| c == 0).next().unwrap_or_default()).into_owned();
            let name = name.trim_start_matches("pack:/").trim_end_matches(".dds").to_string();
            let (width, height) = (u16_at(d, at + 0x1C)? as usize, u16_at(d, at + 0x1E)? as usize);
            let fourcc: [u8; 4] = d[at + 0x20..at + 0x24].try_into().unwrap();
            if !matches!(&fourcc, b"DXT1" | b"DXT3" | b"DXT5") {
                return Err(format!("{name}: format {:?} isn't DXT", String::from_utf8_lossy(&fourcc)));
            }
            let levels = d[at + 0x27] as usize;
            let pixels = ptr(r, u32_at(d, at + 0x48)?)?;
            let size: usize = (0..levels).map(|l| level_size(width >> l, height >> l, fourcc)).sum();
            let data = d.get(pixels..pixels + size).ok_or(format!("{name}: pixels out of range"))?.to_vec();
            Ok(Texture { name, width, height, fourcc, levels, data })
        })
        .collect()
}

/// The texture as a VTF 7.2: no low-res thumbnail, mips smallest first.
pub fn write(t: &Texture) -> Vec<u8> {
    let format: u32 = match &t.fourcc {
        b"DXT1" => 13,
        b"DXT3" => 14,
        _ => 15,
    };
    let mut h = vec![0u8; 80];
    h[0..4].copy_from_slice(b"VTF\0");
    h[4..8].copy_from_slice(&7u32.to_le_bytes());
    h[8..12].copy_from_slice(&2u32.to_le_bytes());
    h[12..16].copy_from_slice(&80u32.to_le_bytes());
    h[16..18].copy_from_slice(&(t.width as u16).to_le_bytes());
    h[18..20].copy_from_slice(&(t.height as u16).to_le_bytes());
    // Flags: none (mipmapped, no alpha used); 1 frame from 0.
    h[24..26].copy_from_slice(&1u16.to_le_bytes());
    for k in 0..3 {
        h[32 + k * 4..36 + k * 4].copy_from_slice(&0.5f32.to_le_bytes());
    }
    h[48..52].copy_from_slice(&1.0f32.to_le_bytes());
    h[52..56].copy_from_slice(&format.to_le_bytes());
    h[56] = t.levels as u8;
    h[57..61].copy_from_slice(&u32::MAX.to_le_bytes());
    h[63..65].copy_from_slice(&1u16.to_le_bytes());
    let mut offsets = vec![];
    let mut at = 0;
    for l in 0..t.levels {
        let n = level_size(t.width >> l, t.height >> l, t.fourcc);
        offsets.push((at, n));
        at += n;
    }
    for &(o, n) in offsets.iter().rev() {
        h.extend_from_slice(&t.data[o..o + n]);
    }
    h
}
