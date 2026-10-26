//! NM's world from the map (ROADMAP M5): the static geometry of the map's `.bsp` as a `delirium::collision::Collision`,
//! as the Playroom gives its agents its room's (`world.add`, docs/adr/0008).
//!
//! Lua reads the map's file (`file.Read`, the game's file system) and hands its bytes over (`DMS.MapLoad`). The world
//! is the world model's faces (no sky, water, triggers, tool faces) and its displacements, in metres, each face moved
//! out along its normal by the fork's shell (`SHAPE_INFLATION`, 6.35 mm): Box3D inflates the map's hulls by that,
//! so its floor is that much over the BSP's plane, and NM has to stand where Box3D's bodies do. Lua then names each
//! material's surface (`$surfaceprop`, `DMS.MapSurfaces`), which picks the surface row NM's friction comes from
//! (materials.dat, as the Playroom's CONCRETE). Without a map — an unreadable `.bsp` — agents get the Playroom's floor
//! at the height traced under them (`data::floor_collision`).

use std::collections::HashMap;
use std::ffi::{CStr, c_int};
use std::sync::{Arc, Mutex};

use delirium::collision::Collision;
use delirium::prelude::*;

use crate::data;
use crate::lua::{self, Api, LuaState, TSTRING, TTABLE, native};

pub const METRES_PER_UNIT: f32 = 0.0254;
/// The fork's shell around every hull (`vphysics/src/vphysics/conversion.h`, `SHAPE_INFLATION`): 0.25 units less a
/// hair, in metres.
pub const SHAPE_INFLATION: f32 = (0.25 - 0.0001) * METRES_PER_UNIT;

const LUMP_PLANES: usize = 1;
const LUMP_TEXDATA: usize = 2;
const LUMP_VERTEXES: usize = 3;
const LUMP_TEXINFO: usize = 6;
const LUMP_FACES: usize = 7;
const LUMP_EDGES: usize = 12;
const LUMP_SURFEDGES: usize = 13;
const LUMP_MODELS: usize = 14;
const LUMP_DISPINFO: usize = 26;
const LUMP_DISP_VERTS: usize = 33;
const LUMP_TEXDATA_STRING_DATA: usize = 43;
const LUMP_TEXDATA_STRING_TABLE: usize = 44;

/// Texinfo flags of faces nothing stands on: 2D and 3D sky, water (warp), triggers, nodraw, hint, skip.
const SURF_SKIP: i32 = 0x2 | 0x4 | 0x8 | 0x40 | 0x80 | 0x100 | 0x200;

/// A convex face of the map: its corners (m, the game's frame, out along its normal by the shell), wound
/// counter-clockwise about its outward normal as NM's triangles are (the Playroom's floor and boxes face out); its
/// material; whether it's ground (facing up), which NM gets in the Playroom floor's 1 m squares.
pub struct Face {
    pub points: Vec<Vec3>,
    pub material: u16,
    pub ground: bool,
}

/// The map's geometry for NM.
pub struct MapMesh {
    pub name: String,
    pub faces: Vec<Face>,
    /// Material names (`maps/...` patches stay as they are), lower case.
    pub materials: Vec<String>,
    pub brush_faces: usize,
    pub displacements: usize,
}

/// Faces facing up at least this much are ground (as NM's own `ground` casts take them, `n.z` 0.7).
const GROUND: f32 = 0.7;
/// The Playroom floor's squares and how far it goes from its origin (m): `world::floor_tris(.., 50)`. Further out the
/// ground is cut coarser (`FAR_TILE`), so a big map doesn't become millions of squares.
const TILE: f32 = 1.0;
const TILED: f32 = 50.0;
const FAR_TILE: f32 = 64.0;

/// Where ground from `lo` to `hi` (m, along x or y in NM's frame) is cut: every metre within `TILED` of the origin,
/// every `FAR_TILE` beyond, and at its ends.
fn cuts(lo: f32, hi: f32) -> Vec<f32> {
    let mut c = vec![lo];
    let mut x = lo;
    loop {
        let step = if (-TILED..TILED).contains(&x) { TILE } else { FAR_TILE };
        let mut next = ((x / step).floor() + 1.0) * step;
        // Coarse cuts stop where the fine ones start.
        if x < -TILED && next > -TILED {
            next = -TILED;
        }
        if next >= hi {
            break;
        }
        c.push(next);
        x = next;
    }
    c.push(hi);
    c
}

/// `points` without repeats and without corners on a straight line between their neighbours (Source's faces carry
/// T-junction points, which would make slivers of no area).
fn clean(points: &[Vec3]) -> Vec<Vec3> {
    let mut p: Vec<Vec3> = vec![];
    for &v in points {
        if p.last().is_none_or(|l| l.distance_squared(v) > 1e-8) {
            p.push(v);
        }
    }
    while p.len() > 2 && p[0].distance_squared(p[p.len() - 1]) <= 1e-8 {
        p.pop();
    }
    loop {
        let n = p.len();
        if n < 3 {
            return p;
        }
        let k = (0..n).find(|&i| {
            let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
            (b - a).cross(c - b).length() <= 1e-6 * (b - a).length().max(1e-6) * (c - b).length().max(1e-6) + 1e-12
        });
        match k {
            Some(i) => {
                p.remove(i);
            }
            None => return p,
        }
    }
}

/// A convex polygon's part on the side of the plane `axis` = `at` given by `keep_below` (Sutherland–Hodgman).
fn clip(poly: &[Vec3], axis: usize, at: f32, keep_below: bool) -> Vec<Vec3> {
    let inside = |v: Vec3| if keep_below { v[axis] <= at } else { v[axis] >= at };
    let mut out = vec![];
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if inside(a) {
            out.push(a);
        }
        if inside(a) != inside(b) {
            let t = (at - a[axis]) / (b[axis] - a[axis]);
            let mut v = a.lerp(b, t);
            v[axis] = at;
            out.push(v);
        }
    }
    out
}

struct Map {
    mesh: Arc<MapMesh>,
    /// The surface row of each material (`DMS.MapSurfaces`), CONCRETE's until then.
    rows: Vec<u8>,
    /// NM's worlds made of it, by the origin of NM's frame (mm).
    worlds: Vec<([i32; 3], Arc<Collision>)>,
}

/// The maps read this session, by name: a map comes back with `changelevel`, and its world needn't be read again.
static MAPS: Mutex<Option<HashMap<String, Arc<MapMesh>>>> = Mutex::new(None);
/// The current map's world.
static MAP: Mutex<Option<Map>> = Mutex::new(None);

fn lump(b: &[u8], i: usize) -> Result<&[u8], String> {
    let h = 8 + i * 16;
    let (ofs, len) = (le_i32(b, h)? as usize, le_i32(b, h + 4)? as usize);
    let data = b.get(ofs..ofs + len).ok_or_else(|| format!("lump {i} is past the file's end"))?;
    if data.starts_with(b"LZMA") {
        return Err(format!("lump {i} is LZMA compressed (not read)"));
    }
    Ok(data)
}

fn le_i32(b: &[u8], at: usize) -> Result<i32, String> {
    b.get(at..at + 4).map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "the file is cut short".to_string())
}

fn le_f32(b: &[u8], at: usize) -> Result<f32, String> {
    le_i32(b, at).map(|i| f32::from_bits(i as u32))
}

fn le_u16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| "the file is cut short".to_string())
}

fn le_i16(b: &[u8], at: usize) -> Result<i16, String> {
    le_u16(b, at).map(|u| u as i16)
}

fn vec3(b: &[u8], at: usize) -> Result<Vec3, String> {
    Ok(Vec3::new(le_f32(b, at)?, le_f32(b, at + 4)?, le_f32(b, at + 8)?))
}

/// Reads the world of a `.bsp` (Source's VBSP, versions 19 to 21).
pub fn parse(name: &str, b: &[u8]) -> Result<MapMesh, String> {
    if b.get(0..4) != Some(b"VBSP") {
        return Err("not a Source map (no VBSP)".into());
    }
    let version = le_i32(b, 4)?;
    if !(19..=21).contains(&version) {
        return Err(format!("map version {version} (19 to 21 are read)"));
    }
    let planes = lump(b, LUMP_PLANES)?;
    let texdata = lump(b, LUMP_TEXDATA)?;
    let verts = lump(b, LUMP_VERTEXES)?;
    let texinfo = lump(b, LUMP_TEXINFO)?;
    let faces = lump(b, LUMP_FACES)?;
    let edges = lump(b, LUMP_EDGES)?;
    let surfedges = lump(b, LUMP_SURFEDGES)?;
    let models = lump(b, LUMP_MODELS)?;
    let dispinfo = lump(b, LUMP_DISPINFO)?;
    let dispverts = lump(b, LUMP_DISP_VERTS)?;
    let strings = lump(b, LUMP_TEXDATA_STRING_DATA)?;
    let table = lump(b, LUMP_TEXDATA_STRING_TABLE)?;

    let vertex = |i: usize| vec3(verts, i * 12);
    // A face's corners, in its winding (`dface_t`, 56 bytes; `surfedges` sign picks the edge's end).
    let corners = |first: usize, n: usize| -> Result<Vec<Vec3>, String> {
        (first..first + n)
            .map(|k| {
                let e = le_i32(surfedges, k * 4)?;
                let (edge, end) = if e >= 0 { (e as usize, 0) } else { (e.unsigned_abs() as usize, 1) };
                vertex(le_u16(edges, edge * 4 + end * 2)? as usize)
            })
            .collect()
    };
    let mut names: Vec<String> = vec![];
    let mut by_texdata: HashMap<i32, u16> = HashMap::new();
    let mut material = |td: i32| -> Result<u16, String> {
        if let Some(&m) = by_texdata.get(&td) {
            return Ok(m);
        }
        let id = le_i32(texdata, td as usize * 32 + 12)?;
        let at = le_i32(table, id as usize * 4)? as usize;
        let s = strings.get(at..).unwrap_or_default();
        let s = &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())];
        let n = String::from_utf8_lossy(s).to_ascii_lowercase().replace('\\', "/");
        let m = names.iter().position(|x| *x == n).unwrap_or_else(|| {
            names.push(n);
            names.len() - 1
        }) as u16;
        by_texdata.insert(td, m);
        Ok(m)
    };

    let mut out_faces: Vec<Face> = vec![];
    let (first_face, face_count) = (le_i32(models, 40)? as usize, le_i32(models, 44)? as usize);
    let (mut nfaces, mut ndisps) = (0, 0);
    // A polygon in metres, wound about `normal` (Source winds its faces the other way round).
    let push = |points: Vec<Vec3>, normal: Vec3, material: u16, out_faces: &mut Vec<Face>| {
        let mut p = clean(&points.iter().map(|&v| v * METRES_PER_UNIT).collect::<Vec<_>>());
        if p.len() < 3 {
            return;
        }
        let mut area: Vec3 = (1..p.len() - 1).map(|i| (p[i] - p[0]).cross(p[i + 1] - p[0])).sum();
        if area.length_squared() < 1e-12 {
            return;
        }
        if area.dot(normal) < 0.0 {
            p.reverse();
            area = -area;
        }
        out_faces.push(Face { points: p, material, ground: area.normalize().z >= GROUND });
    };
    for f in first_face..first_face + face_count {
        let at = f * 56;
        let plane = le_u16(faces, at)? as usize;
        let side = *faces.get(at + 2).ok_or("the faces are cut short")?;
        let (first, n) = (le_i32(faces, at + 4)? as usize, le_i16(faces, at + 8)?.max(0) as usize);
        let (ti, disp) = (le_i16(faces, at + 10)?, le_i16(faces, at + 12)?);
        if ti < 0 || n < 3 {
            continue;
        }
        let flags = le_i32(texinfo, ti as usize * 72 + 64)?;
        if flags & SURF_SKIP != 0 {
            continue;
        }
        let m = material(le_i32(texinfo, ti as usize * 72 + 68)?)?;
        let normal = vec3(planes, plane * 20)? * if side != 0 { -1.0 } else { 1.0 };
        let shell = normal * (SHAPE_INFLATION / METRES_PER_UNIT);
        let c = corners(first, n)?;
        if disp >= 0 {
            // A displacement: its face's four corners from the one nearest its start, a (2^power + 1)^2 grid
            // between them, each point moved along its offset (`CDispVert`: a direction and a length).
            if c.len() != 4 {
                continue;
            }
            let d = disp as usize * 176;
            let start = vec3(dispinfo, d)?;
            let (vstart, power) = (le_i32(dispinfo, d + 12)? as usize, le_i32(dispinfo, d + 20)?);
            if !(1..=4).contains(&power) {
                continue;
            }
            let k0 = (0..4).min_by(|&a, &b| c[a].distance_squared(start).total_cmp(&c[b].distance_squared(start))).unwrap_or(0);
            let p: [Vec3; 4] = std::array::from_fn(|i| c[(k0 + i) % 4]);
            let side_n = (1usize << power) + 1;
            let last = (side_n - 1) as f32;
            let mut grid = Vec::with_capacity(side_n * side_n);
            for r in 0..side_n {
                let (left, right) = (p[0].lerp(p[1], r as f32 / last), p[3].lerp(p[2], r as f32 / last));
                for col in 0..side_n {
                    let v = (vstart + r * side_n + col) * 20;
                    let offset = vec3(dispverts, v)? * le_f32(dispverts, v + 12)?;
                    grid.push(left.lerp(right, col as f32 / last) + offset + shell);
                }
            }
            for r in 0..side_n - 1 {
                for col in 0..side_n - 1 {
                    let k = |r: usize, col: usize| grid[r * side_n + col];
                    let (a, b, cc, dd) = (k(r, col), k(r + 1, col), k(r + 1, col + 1), k(r, col + 1));
                    // Source's alternating diagonals; each triangle faces the face's side.
                    let tris = if (r + col) % 2 == 0 { [[a, b, cc], [a, cc, dd]] } else { [[a, b, dd], [b, cc, dd]] };
                    for t in tris {
                        let n = (t[1] - t[0]).cross(t[2] - t[0]);
                        push(t.to_vec(), if n.dot(normal) >= 0.0 { n } else { -n }, m, &mut out_faces);
                    }
                }
            }
            ndisps += 1;
        } else {
            push(c.iter().map(|&v| v + shell).collect(), normal, m, &mut out_faces);
            nfaces += 1;
        }
    }
    if out_faces.is_empty() {
        return Err("the map's world has no faces NM could stand on".into());
    }
    Ok(MapMesh { name: name.to_string(), faces: out_faces, materials: names, brush_faces: nfaces, displacements: ndisps })
}

/// NM's triangles of the map in a frame at `origin` (m): each face fanned from its first corner, the ground's
/// faces first cut into the Playroom floor's 1 m squares on that frame's grid (each square from its lowest x + y
/// corner, as `world::floor_tris` makes them). Displacements' triangles stay as they are.
pub fn triangles(mesh: &MapMesh, origin: Vec3) -> (Vec<[f32; 3]>, Vec<[u32; 3]>, Vec<u16>) {
    let (mut p, mut t, mut m) = (vec![], vec![], vec![]);
    let mut fan = |poly: &[Vec3], mat: u16| {
        let poly = clean(poly);
        if poly.len() < 3 {
            return;
        }
        let k = p.len() as u32;
        p.extend(poly.iter().map(|v| v.to_array()));
        for i in 1..poly.len() as u32 - 1 {
            t.push([k, k + i, k + i + 1]);
            m.push(mat);
        }
    };
    for f in &mesh.faces {
        let local: Vec<Vec3> = f.points.iter().map(|&v| v - origin).collect();
        if !f.ground {
            fan(&local, f.material);
            continue;
        }
        let (lo, hi) = local.iter().fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        let (xs, ys) = (cuts(lo.x, hi.x), cuts(lo.y, hi.y));
        for x in xs.windows(2) {
            let column = clip(&clip(&local, 0, x[0], false), 0, x[1], true);
            if column.len() < 3 {
                continue;
            }
            for y in ys.windows(2) {
                let mut square = clip(&clip(&column, 1, y[0], false), 1, y[1], true);
                if square.len() < 3 {
                    continue;
                }
                let start = (0..square.len()).min_by(|&a, &b| (square[a].x + square[a].y).total_cmp(&(square[b].x + square[b].y))).unwrap_or(0);
                square.rotate_left(start);
                fan(&square, f.material);
            }
        }
    }
    (p, t, m)
}

/// Source's surface properties (`$surfaceprop`, by what their names start with) and the materials.dat rows that
/// stand for them, the first that's in the game's list (a `*` matches any ending). The rest, and `default`, are
/// CONCRETE, the Playroom's floor.
const SURFACES: &[(&str, &[&str])] = &[
    ("brick", &["BRICK_WALL", "BRICK_COBBLE"]),
    ("concrete", &["CONCRETE"]),
    ("plaster", &["PLASTER_BOARD", "CONCRETE"]),
    ("rock", &["ROCK", "STONE"]),
    ("boulder", &["ROCK", "STONE"]),
    ("stone", &["STONE", "ROCK"]),
    ("gravel", &["GRAVEL", "LOOSE_GRAVEL"]),
    ("asphalt", &["TARMAC"]),
    ("dirt", &["DIRT_DRY", "DIRT*"]),
    ("mud", &["MUD_SOFT", "MUD*"]),
    ("slipperyslime", &["MUD_SOFT", "MUD*"]),
    ("grass", &["GRASS", "SHORT_GRASS"]),
    ("sand", &["SAND*"]),
    ("snow", &["SNOW*"]),
    ("ice", &["ICE*"]),
    ("tile", &["TILES*", "MARBLE"]),
    ("ceiling_tile", &["TILES*"]),
    ("carpet", &["CARPET*"]),
    ("rubber", &["RUBBER*"]),
    ("plastic", &["PLASTIC*"]),
    ("glass", &["GLASS*"]),
    ("wood", &["WOOD*"]),
    ("metal", &["METAL*"]),
    ("chainlink", &["METAL*"]),
    ("solidmetal", &["METAL*"]),
];

/// The surface row a `$surfaceprop` stands for (CONCRETE's for unknown ones), and the rule that picked it.
pub fn surface_row(prop: &str, rows: &[String], concrete: u8) -> u8 {
    let prop = prop.to_ascii_lowercase();
    let wanted = SURFACES.iter().find(|(p, _)| prop.starts_with(p)).map_or(&[][..], |s| s.1);
    wanted
        .iter()
        .find_map(|w| match w.strip_suffix('*') {
            Some(pre) => rows.iter().position(|r| r.starts_with(pre)),
            None => rows.iter().position(|r| r == w),
        })
        .map_or(concrete, |r| r as u8)
}

/// NM's world for an agent whose frame starts at `origin` (m, the game's frame): the current map's, or none.
pub fn collision(origin: Vec3) -> Option<Arc<Collision>> {
    let mut slot = MAP.lock().unwrap_or_else(|e| e.into_inner());
    let map = slot.as_mut()?;
    let key = (origin * 1000.0).round().as_ivec3().to_array();
    if let Some((_, w)) = map.worlds.iter().find(|(k, _)| *k == key) {
        return Some(w.clone());
    }
    let (p, t, mats) = triangles(&map.mesh, origin);
    let m: Vec<u8> = mats.iter().map(|&i| map.rows.get(i as usize).copied().unwrap_or(0)).collect();
    let mut c = Collision::default();
    c.add(&p, &t, &m);
    let c = Arc::new(c);
    // The bench makes a world for each scenario's origin: only a few are kept.
    if map.worlds.len() >= 4 {
        map.worlds.remove(0);
    }
    map.worlds.push((key, c.clone()));
    Some(c)
}

pub const NATIVES: &[(&CStr, lua::CFunction)] = &[(c"MapLoad", map_load), (c"MapSurfaces", map_surfaces), (c"MapUnload", map_unload), (c"MapGround", map_ground)];

unsafe fn push_info(api: &Api, l: *mut LuaState, m: &MapMesh) {
    unsafe {
        (api.createtable)(l, 0, 5);
        api.set_str(l, c"name", &m.name);
        api.set_number(l, c"faces", m.brush_faces as f64);
        api.set_number(l, c"displacements", m.displacements as f64);
        api.set_number(l, c"polygons", m.faces.len() as f64);
        (api.createtable)(l, m.materials.len() as c_int, 0);
        for (i, n) in m.materials.iter().enumerate() {
            api.push_str(l, n);
            api.append(l, i + 1);
        }
        (api.setfield)(l, -2, c"materials".as_ptr());
    }
}

native!(map_load => map_load_impl);
/// `DMS.MapLoad(name, bytes)`: the map's world for NM from its `.bsp` (read once a session per name; `bytes` may be
/// nil for one read before). `{name, faces, displacements, triangles, materials = {name...}}`: name the materials'
/// surfaces next (`DMS.MapSurfaces`). Errors leave agents on a traced floor.
unsafe fn map_load_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let name = unsafe { api.check_string(l, 1)? };
    let mut maps = MAPS.lock().unwrap_or_else(|e| e.into_inner());
    let maps = maps.get_or_insert_with(HashMap::new);
    let mesh = match maps.get(&name) {
        Some(m) => m.clone(),
        None => {
            if unsafe { (api.type_)(l, 2) } != TSTRING {
                return Err(format!("{name} hasn't been read: its .bsp's bytes expected"));
            }
            let bytes = unsafe { api.check_bytes(l, 2)? };
            let m = Arc::new(parse(&name, bytes).map_err(|e| format!("{name}.bsp: {e}"))?);
            maps.insert(name.clone(), m.clone());
            m
        }
    };
    let concrete = data::get().map_or(0, |d| d.floor_material);
    let rows = vec![concrete; mesh.materials.len()];
    *MAP.lock().unwrap_or_else(|e| e.into_inner()) = Some(Map { mesh: mesh.clone(), rows, worlds: vec![] });
    unsafe { push_info(api, l, &mesh) };
    Ok(1)
}

native!(map_surfaces => map_surfaces_impl);
/// `DMS.MapSurfaces({surfaceprop...})`: the `$surfaceprop` of each of `DMS.MapLoad`'s materials, in its order (nil or
/// "" for none: CONCRETE). `{surface row name...}` as they came out. Needs NM's data (materials.dat).
unsafe fn map_surfaces_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    if unsafe { (api.type_)(l, 1) } != TTABLE {
        return Err("bad argument #1 (a table of surface properties expected)".into());
    }
    let d = data::get().ok_or("NM's data isn't loaded")?;
    let mut slot = MAP.lock().unwrap_or_else(|e| e.into_inner());
    let map = slot.as_mut().ok_or("no map loaded (DMS.MapLoad)")?;
    let rows: Vec<u8> = (0..map.mesh.materials.len())
        .map(|i| unsafe {
            (api.rawgeti)(l, 1, i as c_int + 1);
            let prop = if (api.type_)(l, -1) == TSTRING { api.check_string(l, -1).unwrap_or_default() } else { String::new() };
            (api.settop)(l, -2);
            surface_row(&prop, &d.surface_names, d.floor_material)
        })
        .collect();
    map.rows = rows;
    map.worlds.clear();
    unsafe {
        (api.createtable)(l, map.rows.len() as c_int, 0);
        for (i, &r) in map.rows.iter().enumerate() {
            api.push_str(l, d.surface_names.get(r as usize).map_or("?", String::as_str));
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

native!(map_unload => map_unload_impl);
/// `DMS.MapUnload()`: no map world (new agents get a traced floor).
unsafe fn map_unload_impl(_api: &Api, _l: *mut LuaState) -> Result<c_int, String> {
    *MAP.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(0)
}

native!(map_ground => map_ground_impl);
/// `DMS.MapGround(pos[, len])`: where NM's map world is under `pos` (m, the game's frame), looking down `len` m
/// (default 10): `z, surface row name`, or nil. For checking NM's world against the game's traces.
unsafe fn map_ground_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let p = Vec3::from_array(unsafe { api.check_vec3(l, 1)? });
    let len = unsafe { api.opt_number(l, 2, 10.0)? } as f32;
    let w = collision(Vec3::ZERO).ok_or("no map loaded (DMS.MapLoad)")?;
    match w.ground_material(p, len) {
        Some((z, m)) => unsafe {
            (api.pushnumber)(l, f64::from(z));
            let name = data::get().and_then(|d| d.surface_names.get(m as usize).cloned()).unwrap_or_default();
            api.push_str(l, &name);
            Ok(2)
        },
        None => {
            unsafe { (api.pushnil)(l) };
            Ok(1)
        }
    }
}
