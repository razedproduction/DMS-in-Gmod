//! NM's inputs, straight out of the user's copy of GTA IV (`dms_game_dir`), as the Playroom takes them with
//! `--game` (docs/adr/0008, 0009): fred.xrt, taskparams.txt (if the game has one), materials.dat's surfaces and
//! Homer's build from `m_y_pmanhat_01`. Nothing is copied out of the game.
//!
//! Delirium keeps the character in process-wide state that's set once, so the first game loaded stays.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::nm;

/// What was loaded, and from where.
pub struct Data {
    pub dir: PathBuf,
    pub platform: &'static str,
    pub build: nm::Build,
    /// The build as NM's physics has it, for the ragdoll (`bind.rs`).
    pub body: dms_game::body::Body,
    /// The build as the Playroom's `NM_DUMP` writes it.
    pub dump: String,
    /// CONCRETE's surface row, the Playroom's floor.
    pub floor_material: u8,
    pub surfaces: usize,
    /// Where taskparams.txt came from, or why NM runs its defaults.
    pub taskparams: String,
}

static DATA: Mutex<Option<Arc<Data>>> = Mutex::new(None);

/// The loaded data, if any.
pub fn get() -> Option<Arc<Data>> {
    DATA.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// What to tell someone who hasn't set the game's folder.
pub fn how_to() -> String {
    "set dms_game_dir to your GTA IV folder (PC: the one with GTAIV.exe and pc\\; Xbox 360: the one with \
     default.xex), e.g.  dms_game_dir \"D:\\Games\\Grand Theft Auto IV\"  (the game's files are read from there, \
     nothing is copied)"
        .into()
}

/// Loads the data from the game at `dir` once; later calls give the same data (and fail for another folder).
pub fn load(dir: &str) -> Result<Arc<Data>, String> {
    let dir = dir.trim().trim_matches('"');
    if dir.is_empty() {
        return Err(format!("no game folder: {}", how_to()));
    }
    let mut slot = DATA.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = slot.as_ref() {
        let same = std::fs::canonicalize(dir).ok() == std::fs::canonicalize(&d.dir).ok();
        return if same {
            Ok(d.clone())
        } else {
            Err(format!("NM's data is already loaded from {} (Delirium sets it once): restart to use {dir}", d.dir.display()))
        };
    }
    let d = Arc::new(read(dir).map_err(|e| format!("{e}\n  {}", how_to()))?);
    *slot = Some(d.clone());
    Ok(d)
}

/// The Playroom's `load` order: the platform (and the PC's byte order), materials.dat's surfaces, fred.xrt,
/// Homer's build (with the links it derives), taskparams.txt.
fn read(dir: &str) -> Result<Data, String> {
    let game = dms_game::Game::open(dir)?;
    let materials = game.materials();
    let rows = materials.as_deref().map(dms_game::surfaces).unwrap_or_default();
    if let Some(m) = materials.as_deref() {
        dms_game::set_surfaces(m);
    }
    let fred = game.fred()?;
    nm::init_memory(&fred)?;
    let (build, frames) = game.homer()?;
    let dump = build.dump(&frames);
    let taskparams = match game.taskparams() {
        Some(text) => {
            // The SDK reads them from a file; the game's may be inside an archive.
            let tmp = std::env::temp_dir().join(format!("dms-taskparams-{}.txt", std::process::id()));
            std::fs::write(&tmp, &text).map_err(|e| format!("{}: {e}", tmp.display()))?;
            let r = nm::load_task_params(&tmp);
            let _ = std::fs::remove_file(&tmp);
            r?;
            "data/taskparams.txt".to_string()
        }
        None => "none in this game (the PC version has none): NM's defaults, as in the Playroom".to_string(),
    };
    Ok(Data {
        dir: PathBuf::from(dir),
        platform: match game.platform() {
            dms_game::Platform::Pc => "PC",
            dms_game::Platform::Xbox360 => "Xbox 360",
        },
        build: nm::Build::from_text(&dump)?,
        body: dms_game::body::Body::new(&build, &frames),
        dump,
        floor_material: dms_game::concrete(&rows),
        surfaces: rows.len(),
        taskparams,
    })
}

/// The Playroom room's floor (`world::floor_tris(pool, 50)`, CONCRETE) as an NM world.
pub fn floor_world(d: &Data) -> Result<nm::World, String> {
    let (p, t) = dms_game::floor(true, 50.0);
    let mut w = nm::World::new();
    w.add_mesh(&p, &t, &vec![d.floor_material; t.len()])?;
    Ok(w)
}
