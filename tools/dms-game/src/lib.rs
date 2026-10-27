//! NM's inputs from the user's copy of GTA IV, read the way the Playroom (`delirium-room`) reads them
//! (docs/adr/0008, 0009): the same files, the same `delirium-data` calls, in the same order.
//!
//! Mirrors `delirium-room`'s `archive_read`, `load` and `getup::{key, plat, homer, Clips::stand_up}`, and the get-up
//! clips, weapons and agility its reactions use (`getup.rs`, ROADMAP M6). Only what Homer's agent needs is read;
//! anything else of the Playroom's (the room, the fights' clips) isn't.

pub mod body;
pub mod getup;
pub mod stage;

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use delirium::prelude::*;
use delirium::ragdoll::{Build, Rig};
use delirium_data::archive_key::ArchiveKey;
use delirium_data::{anim, img, model, resource, rpf, skeleton};

/// Homer: the ped whose fragment the Playroom builds his body from.
pub const HOMER_MODEL: &str = "m_y_pmanhat_01";
/// Which NM character Homer is (`delirium::nmrs::HOMER`, fred.xrt).
pub const HOMER_NM: usize = delirium::nmrs::HOMER;
/// The version of Homer's model (`tools/dms-homer`): one more whenever what it writes changes (the body, its bones,
/// the `.phy`). dms-homer marks the model with it (`models/dms/homer_version.txt`), the module asks for it at start.
pub const MODEL_VERSION: u32 = 1;

/// The game's platform, as the Playroom tells them apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Pc,
    Xbox360,
}

/// An opened copy of the game: its folder, platform and archive key.
pub struct Game {
    dir: PathBuf,
    platform: Platform,
    key: [u8; 32],
}

/// Whether `dir` is a game folder the Playroom takes (its `is_game`, less PS3, which isn't supported here).
pub fn is_game(dir: &Path) -> bool {
    dir.join("default.xex").is_file() || dir.join("GTAIV.exe").is_file() && dir.join("pc").is_dir()
}

impl Game {
    /// Opens the game at `dir` and finds its archive key. Sets `delirium_data::LE` for the PC's little-endian
    /// files, as the Playroom's `getup::key` does: call it before reading anything.
    pub fn open(dir: impl AsRef<Path>) -> Result<Game, String> {
        let dir = dir.as_ref().to_path_buf();
        if dir.join("ps3.rpf").is_file() {
            return Err(format!("{}: the PS3 version isn't supported, use the PC or Xbox 360 one", dir.display()));
        }
        if !is_game(&dir) {
            return Err(format!(
                "{}: not GTA IV's folder (want GTAIV.exe and pc\\ for the PC version, default.xex for the Xbox 360 one)",
                dir.display()
            ));
        }
        let platform = if dir.join("GTAIV.exe").is_file() { Platform::Pc } else { Platform::Xbox360 };
        delirium_data::LE.store(platform == Platform::Pc, std::sync::atomic::Ordering::Relaxed);
        let key = find_key(&dir, platform)?;
        Ok(Game { dir, platform, key })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    fn key(&self) -> ArchiveKey {
        ArchiveKey::from_bytes(&self.key)
    }

    /// An Xbox 360 path as the platform has it (`getup::plat`).
    pub fn plat(&self, s: &str) -> String {
        match self.platform {
            Platform::Pc => [("xbox360", "pc"), (".xrt", ".art"), (".xft", ".wft"), (".xad", ".wad"), (".xtd", ".wtd"), (".xdr", ".wdr")]
                .iter()
                .fold(s.into(), |s: String, (a, b)| s.replace(a, b)),
            Platform::Xbox360 => s.into(),
        }
    }

    /// A file out of `rpf` (`common.rpf` or `xbox360.rpf`), as the Playroom's `archive_read` finds it: on PC,
    /// loose under `common\` or `pc\`.
    pub fn archive(&self, rpf_name: &str, path: &str) -> Result<Vec<u8>, String> {
        if self.platform == Platform::Pc {
            let sub = if rpf_name == "common.rpf" { "common" } else { "pc" };
            let file = self.dir.join(sub).join(self.plat(path));
            return std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()));
        }
        let file = self.dir.join(self.plat(rpf_name));
        let data = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let toc = rpf::read_toc(&data, &self.key()).unwrap_or_default();
        let want = self.plat(path);
        let e = toc.iter().find(|e| e.path.eq_ignore_ascii_case(&want)).ok_or(format!("{want}: not in {rpf_name}"))?;
        rpf::read(&data, e)
    }

    /// NM's character, `data/naturalmotion/fred.xrt` (`Fred.art` on PC).
    pub fn fred(&self) -> Result<Vec<u8>, String> {
        self.archive("xbox360.rpf", "data/naturalmotion/fred.xrt")
    }

    /// The tasks' tuning, `data/taskparams.txt`; none if the game hasn't one (the PC version), and then the
    /// Playroom runs NM's defaults.
    pub fn taskparams(&self) -> Option<String> {
        self.archive("common.rpf", "data/taskparams.txt").ok().map(|d| String::from_utf8_lossy(&d).into_owned())
    }

    /// `data/materials/materials.dat`, the surfaces' table.
    pub fn materials(&self) -> Option<String> {
        self.archive("common.rpf", "data/materials/materials.dat").ok().map(|d| String::from_utf8_lossy(&d).into_owned())
    }

    /// `data/weaponinfo.xml`'s weapons, as the Playroom's `load` reads them (none without the file).
    pub fn weapons(&self) -> Vec<delirium_data::weapons::Weapon> {
        self.archive("common.rpf", "data/weaponinfo.xml")
            .ok()
            .map(|d| delirium_data::weapons::read(&String::from_utf8_lossy(&d)))
            .unwrap_or_default()
    }

    /// Homer's agility, `data/pedpersonality.dat`'s column 7 for his model (the Playroom's `load`; 3 without it).
    pub fn agility(&self) -> u8 {
        self.archive("common.rpf", "data/pedpersonality.dat")
            .ok()
            .map(|d| String::from_utf8_lossy(&d).into_owned())
            .and_then(|t| {
                t.lines()
                    .map(|l| l.split(',').map(str::trim).collect::<Vec<_>>())
                    .find(|c| c.len() > 7 && c[0].eq_ignore_ascii_case(HOMER_MODEL))
                    .and_then(|c| c[7].parse::<u8>().ok())
            })
            .unwrap_or(3)
    }

    /// Homer's build and its standing frames, as the Playroom makes them with `--game`: `getup::homer` (which
    /// also sets `delirium::nmdbm::DERIVED_LINKS` from the idle clip) and then `Clips::stand_up`.
    /// fred.xrt has to be loaded first (`nm_init_memory`), as in the Playroom.
    pub fn homer(&self) -> Result<(Build, Vec<Transform>), String> {
        if delirium::nmrs::ASSET[HOMER_NM].get().is_none() {
            return Err("fred.xrt isn't loaded yet (nm_init_memory first)".into());
        }
        let idle = self.idle()?;
        let mut b = self.homer_build()?;
        let links_set = delirium::nmdbm::logged_links().is_some() || delirium::nmdbm::DERIVED_LINKS[HOMER_NM].get().is_some();
        if !links_set {
            let turn = Transform::from_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2));
            let stand: Vec<Quat> = idle.place(&b, turn)?.iter().map(|f| f.rotation).collect();
            if let Some(l) = delirium::nmrs::derive_links(&b, &stand) {
                let _ = delirium::nmdbm::DERIVED_LINKS[HOMER_NM].set(l);
                b = self.homer_build()?;
            }
        }
        let frames = idle.stand_up(&mut b)?;
        Ok((b, frames))
    }

    /// Homer's fragment (`componentpeds.img/m_y_pmanhat_01.xft`), the one his build is made from: skeleton,
    /// ragdoll parts, meshes and embedded textures.
    pub fn homer_fragment(&self) -> Result<resource::Resource, String> {
        let (table, mut f) = self.img_open("xbox360/models/cdimages/componentpeds.img")?;
        self.img_read(&table, &mut f, &format!("{HOMER_MODEL}.xft"))
    }

    /// `getup::homer_build`: the ped's fragment through the same `Rig` Delirium builds Homer with.
    fn homer_build(&self) -> Result<Build, String> {
        let frag = self.homer_fragment()?;
        let bones = skeleton::read(&frag, model::fragment_root(&frag)?)?;
        let parts = delirium_data::ragdoll::read(&frag)?;
        let parents: Vec<Option<usize>> = bones.iter().map(|b| b.parent).collect();
        let bind: Vec<(Vec3, Quat)> = bones.iter().map(|b| (Vec3::from(b.translation), Quat::from_array(b.rotation).normalize())).collect();
        Rig::new(&parts, &parents, &bind, &bones, HOMER_NM).map(|r| r.build).ok_or(format!("{HOMER_MODEL}.xft: no ragdoll"))
    }

    /// The player's skeleton and `move_player`'s `idle`, the part of `getup::Clips::load` that `homer` and
    /// `stand_up` use.
    fn idle(&self) -> Result<Idle, String> {
        let peds = match self.platform {
            Platform::Pc => {
                let p = self.dir.join("pc/models/cdimages/playerped.rpf");
                std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?
            }
            Platform::Xbox360 => {
                let p = self.dir.join("xbox360.rpf");
                let platform = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                let toc = rpf::read_toc(&platform, &self.key())?;
                let nested = toc.iter().find(|e| e.path.eq_ignore_ascii_case("models/cdimages/playerped.rpf")).ok_or("playerped.rpf not found")?;
                rpf::read(&platform, nested)?
            }
        };
        let inner = rpf::read_toc(&peds, &self.key())?;
        let e = inner.iter().find(|e| e.path.eq_ignore_ascii_case(&self.plat("player.xft"))).ok_or("player.xft not in playerped.rpf")?;
        let frag = resource::parse(&rpf::read(&peds, e)?)?;
        let bones = skeleton::read(&frag, model::fragment_root(&frag)?)?;
        let (table, mut f) = self.img_open("xbox360/anim/anim.img")?;
        let (dict, name) = ("move_player.xad", "idle");
        let clip = anim::read_dictionary(&self.img_read(&table, &mut f, dict)?)?
            .into_iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .ok_or(format!("no {name} in {dict}"))?;
        let map = clip.tracks.iter().map(|t| bones.iter().position(|b| b.id == t.bone).filter(|&b| b != 0)).collect();
        Ok(Idle {
            parents: bones.iter().map(|b| b.parent).collect(),
            bind: bones.iter().map(|b| (Vec3::from(b.translation), Quat::from_array(b.rotation).normalize())).collect(),
            names: bones.iter().map(|b| b.name.clone()).collect(),
            clip,
            map,
        })
    }

    fn img_open(&self, path: &str) -> Result<(Vec<img::Entry>, std::fs::File), String> {
        let path = self.dir.join(self.plat(path));
        let name = path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let mut f = std::fs::File::open(&path).map_err(|e| format!("{name}: {e}"))?;
        let mut head = vec![];
        (&mut f).take(4 << 20).read_to_end(&mut head).map_err(|e| e.to_string())?;
        Ok((img::read_table(&head, Some(&self.key()))?, f))
    }

    fn img_read(&self, table: &[img::Entry], f: &mut std::fs::File, name: &str) -> Result<resource::Resource, String> {
        let name = &self.plat(name);
        let e = table.iter().find(|e| e.name.eq_ignore_ascii_case(name)).ok_or(format!("{name} not found"))?;
        let mut raw = vec![];
        f.seek(SeekFrom::Start(e.offset)).map_err(|e| e.to_string())?;
        f.take(e.used_blocks as u64 * 2048).read_to_end(&mut raw).map_err(|e| e.to_string())?;
        resource::parse(&raw)
    }
}

/// `getup::key`, without fetching anything: the Xbox 360's xex key has to be where the Playroom keeps it.
fn find_key(dir: &Path, platform: Platform) -> Result<[u8; 32], String> {
    let head = |p: PathBuf| -> Result<Vec<u8>, String> {
        let mut head = vec![0; 0x810];
        std::fs::File::open(&p).and_then(|mut f| f.read_exact(&mut head)).map_err(|e| format!("{}: {e}", p.display()))?;
        Ok(head)
    };
    match platform {
        Platform::Pc => {
            let exe = std::fs::read(dir.join("GTAIV.exe")).map_err(|e| format!("GTAIV.exe: {e}"))?;
            let head = head(dir.join("pc/models/cdimages/playerped.rpf"))?;
            ArchiveKey::find_by_trial(&exe, &head).map(|(_, k)| k).ok_or_else(|| "no archive key in GTAIV.exe".into())
        }
        Platform::Xbox360 => {
            let keyfile = config_dir().join("xex.key");
            let k: [u8; 16] = std::fs::read(&keyfile).ok().and_then(|b| b.try_into().ok()).ok_or_else(|| {
                format!("{}: no xex key (run the Playroom once with this game: it fetches it there)", keyfile.display())
            })?;
            let xex = std::fs::read(dir.join("default.xex")).map_err(|e| format!("default.xex: {e}"))?;
            let xex = delirium_data::xex::XexImage::parse_with_key(&xex, &k).map_err(|e| format!("default.xex: {e}"))?;
            let head = head(dir.join("xbox360.rpf"))?;
            ArchiveKey::find_by_trial(&xex.image, &head).map(|(_, k)| k).ok_or_else(|| "no archive key in default.xex".into())
        }
    }
}

/// The Playroom's `getup::config_dir`.
fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_default()
        .join("delirium")
}

/// The idle clip on the player's skeleton.
struct Idle {
    parents: Vec<Option<usize>>,
    bind: Vec<(Vec3, Quat)>,
    names: Vec<String>,
    clip: anim::Animation,
    map: Vec<Option<usize>>,
}

impl Idle {
    /// `Clips::pose(IDLE, 0.0)`.
    fn pose(&self) -> Vec<(Vec3, Quat)> {
        let (a, map) = (&self.clip, &self.map);
        let x = 0.0f32.clamp(0.0, 1.0) * (a.frames - 1) as f32;
        let (i, f) = (x.floor() as usize, x.fract());
        let q = |v: &[f32]| Quat::from_xyzw(v[0], v[1], v[2], v[3]).normalize();
        let mut out = self.bind.clone();
        out[0] = (Vec3::ZERO, Quat::IDENTITY);
        for (t, bone) in a.tracks.iter().zip(map) {
            let (va, vb) = (t.sample(i), t.sample(i + 1));
            match (t.kind, bone, t.bone) {
                (anim::BONE_ROTATION, Some(b), _) => out[*b].1 = q(&va).slerp(q(&vb), f),
                (anim::BONE_TRANSLATION, Some(b), _) => out[*b].0 = Vec3::from_slice(&va).lerp(Vec3::from_slice(&vb), f),
                (anim::BONE_ROTATION, None, 0) => out[0].1 = q(&va).slerp(q(&vb), f),
                (anim::BONE_TRANSLATION | anim::MOVER_TRANSLATION, None, 0) => {
                    out[0].0 += Vec3::from_slice(&va).lerp(Vec3::from_slice(&vb), f)
                }
                _ => {}
            }
        }
        out
    }

    /// `Clips::place` of the idle pose for `build`'s parts (`Clips::load`'s part map; no `grow` scale).
    fn place(&self, build: &Build, body: Transform) -> Result<Vec<Transform>, String> {
        let parts: Vec<(usize, Transform)> = build
            .parts
            .iter()
            .map(|p| self.names.iter().position(|n| *n == p.name).map(|b| (b, p.offset)).ok_or(format!("no bone {}", p.name)))
            .collect::<Result<_, _>>()?;
        let pose = self.pose();
        let mut bones: Vec<(Vec3, Quat)> = Vec::with_capacity(pose.len());
        for (i, &(t, r)) in pose.iter().enumerate() {
            let (pt, pr) = self.parents[i].map_or((body.translation, body.rotation), |p| bones[p]);
            bones.push((pt + pr * t * 1.0, pr * r));
        }
        Ok(parts
            .iter()
            .map(|&(b, o)| {
                let (bt, br) = bones[b];
                Transform::from_translation(bt + br * o.translation * 1.0).with_rotation(br * o.rotation)
            })
            .collect())
    }

    /// `Clips::stand_up`.
    fn stand_up(&self, build: &mut Build) -> Result<Vec<Transform>, String> {
        let mut frames = self.place(build, Transform::from_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)))?;
        for (p, f) in build.parts.iter_mut().zip(&frames) {
            p.bind = f.rotation;
        }
        delirium::nmrs::asset_layout(build, &mut frames);
        Ok(frames)
    }
}

/// Puts materials.dat's friction and elasticity per row in `delirium::ragdoll::SURFACES`, as the Playroom's
/// `load` does (the whole table at once: `nm_set_surface` would refuse rows out of its ranges). Returns the rows.
pub fn set_surfaces(materials: &str) -> usize {
    let f = delirium_data::vehicles::read_friction(materials);
    let n = f.len();
    *delirium::ragdoll::SURFACES.write().unwrap_or_else(|e| e.into_inner()) = f;
    n
}

/// The surfaces' rows by name, as the Playroom's `load` lists them (for finding CONCRETE's).
pub fn surfaces(materials: &str) -> Vec<(String, f32, f32)> {
    let f = delirium_data::vehicles::read_friction(materials);
    materials
        .lines()
        .filter(|l| l.starts_with(|c: char| c.is_ascii_alphabetic()))
        .zip(f)
        .map(|(l, [fr, el])| (l.split_whitespace().next().unwrap_or("").to_string(), fr, el))
        .collect()
}

/// The Playroom room's pool, sunk in its floor (`world::POOL`): x from -3 to 3, y from 5 to 25 m.
pub const POOL: [[f32; 2]; 2] = [[-3.0, 5.0], [3.0, 25.0]];

/// The Playroom's floor rectangles (`world::floor_rects`): the whole square, or four around the pool.
fn floor_rects(pool: bool, half: f32) -> Vec<([f32; 2], [f32; 2])> {
    let ([a, b], h) = (POOL, half);
    if !pool {
        return vec![([-h, -h], [h, h])];
    }
    vec![([-h, -h], [h, a[1]]), ([-h, b[1]], [h, h]), ([-h, a[1]], [a[0], b[1]]), ([b[0], a[1]], [h, b[1]])]
}

/// The Playroom's floor, `world::floor_tris(room.pool, 50)` with CONCRETE's row in `load`: 1 m squares at z = 0,
/// with a hole for the pool when `pool` (the room has one; the stages haven't). For `nm_world_add_mesh`.
pub fn floor(pool: bool, half: f32) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let (mut p, mut t) = (vec![], vec![]);
    for (lo, hi) in floor_rects(pool, half) {
        for x in lo[0] as i32..hi[0] as i32 {
            for y in lo[1] as i32..hi[1] as i32 {
                let (a, b, k) = (x as f32, y as f32, p.len() as u32);
                p.extend([[a, b, 0.0], [a + 1.0, b, 0.0], [a + 1.0, b + 1.0, 0.0], [a, b + 1.0, 0.0]]);
                t.extend([[k, k + 1, k + 2], [k, k + 2, k + 3]]);
            }
        }
    }
    (p, t)
}

/// CONCRETE's row in the surfaces' table (the Playroom's floor), 0 without one.
pub fn concrete(surfaces: &[(String, f32, f32)]) -> u8 {
    surfaces.iter().position(|s| s.0 == "CONCRETE").unwrap_or(0) as u8
}

