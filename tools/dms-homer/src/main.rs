//! dms-homer: makes `garrysmod/models/dms/homer.mdl` (and `.phy`, materials) from the user's copy of GTA IV, the
//! body NM's own (ROADMAP M3, docs/adr/0013). The model and everything made from the game stay on this machine.

use std::path::{Path, PathBuf};
use std::process::Command;

use dms_homer::{phy, smd};

const USAGE: &str = "\
usage: dms-homer [--game <GTA IV dir>] [--gmod <GarrysMod dir>] [--work <dir>]
  makes garrysmod\\models\\dms\\homer.mdl (+ .phy, .vvd, .vtx) from the game's Homer (m_y_pmanhat_01): NM's body,
  drawn as the Playroom draws it, with studiomdl from Garry's Mod's bin\\win64, and checks the compiled .phy against NM's body.
  --game  GTA IV's folder (default: DMS_GAME_DIR, else golden\\game_dir.txt)
  --gmod  Garry's Mod's folder (default: DMS_GMOD_DIR, else Steam's library from the registry)
  --work  where the SMDs, the .qc and studiomdl's log go (default: target\\dms-homer; never committed)";

fn main() {
    if let Err(e) = run() {
        eprintln!("dms-homer: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let mut take = |flag: &str| {
        args.iter().position(|a| a == flag).map(|i| {
            let v = args.get(i + 1).cloned().unwrap_or_default();
            args.drain(i..(i + 2).min(args.len()));
            PathBuf::from(v)
        })
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).ok_or("no repository")?.to_path_buf();
    let game = take("--game")
        .or_else(|| std::env::var_os("DMS_GAME_DIR").map(PathBuf::from))
        .or_else(|| std::fs::read_to_string(repo.join("golden").join("game_dir.txt")).ok().map(|t| PathBuf::from(t.trim())))
        .ok_or("no game folder: pass --game <your GTA IV folder>")?;
    let gmod = take("--gmod").or_else(|| std::env::var_os("DMS_GMOD_DIR").map(PathBuf::from)).or_else(dms_homer::steam_gmod)
        .ok_or("Garry's Mod not found: pass --gmod <GarrysMod folder>")?;
    let work = take("--work").unwrap_or_else(|| repo.join("target").join("dms-homer"));
    if !args.is_empty() {
        return Err(format!("unknown arguments {args:?}\n{USAGE}"));
    }
    let studiomdl = gmod.join("bin").join("win64").join("studiomdl.exe");
    let garrysmod = gmod.join("garrysmod");
    if !studiomdl.is_file() || !garrysmod.is_dir() {
        return Err(format!("{}: not Garry's Mod's folder (want bin\\win64\\studiomdl.exe and garrysmod\\)", gmod.display()));
    }

    println!("dms-homer: Homer's body from {}", game.display());
    let body = homer_body(&game)?;
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let bones = dms_homer::bones(&body);
    let parts = dms_homer::part_frames(&body);
    let hulls = dms_homer::hulls(&body);
    let write = |name: &str, text: &str| std::fs::write(work.join(name), text).map_err(|e| format!("{name}: {e}"));
    write("homer_phys.smd", &smd::write(&bones, &dms_homer::hull_triangles(&parts, &hulls, "phys")))?;
    // The Playroom's look: NM's capsules and boxes in its colours, its other colour schemes as skins.
    let materials = garrysmod.join("materials").join(dms_homer::MATERIALS.replace('/', "\\"));
    std::fs::create_dir_all(&materials).map_err(|e| format!("{}: {e}", materials.display()))?;
    for m in dms_homer::visual::BASE.iter().chain(dms_homer::visual::SKINS.iter().flat_map(|(_, ms)| ms.iter())) {
        std::fs::write(materials.join(format!("{}.vmt", m.name)), dms_homer::visual::vmt(m)).map_err(|e| e.to_string())?;
    }
    write("homer_ref.smd", &smd::write(&bones, &dms_homer::visual::triangles(&body, &parts)))?;
    write("homer.qc", &dms_homer::qc(&body, "homer_ref.smd", "homer_phys.smd"))?;

    println!("dms-homer: studiomdl ({})", studiomdl.display());
    let log = work.join("studiomdl.log");
    let out = Command::new(&studiomdl)
        .current_dir(&work)
        .arg("-game").arg(&garrysmod).args(["-nop4", "-verbose"]).arg(work.join("homer.qc"))
        .output().map_err(|e| format!("studiomdl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    std::fs::write(&log, &text).map_err(|e| e.to_string())?;
    let mdl = garrysmod.join("models").join(dms_homer::MODEL.replace('/', "\\"));
    if !out.status.success() || text.contains("ERROR") {
        let tail: Vec<&str> = text.lines().filter(|l| l.contains("ERROR") || l.contains("WARNING")).take(10).collect();
        return Err(format!("studiomdl failed ({}): see {}\n{}", out.status, log.display(), tail.join("\n")));
    }
    let phy_path = mdl.with_extension("phy");
    let data = std::fs::read(&phy_path).map_err(|e| format!("{}: {e}", phy_path.display()))?;
    let mut compiled = phy::read(&data)?;
    std::fs::write(work.join("homer.studiomdl.phy.txt"), &compiled.text).map_err(|e| e.to_string())?;
    dms_homer::set_mass_properties(&body, &mut compiled)?;
    let text = dms_homer::phy_text(&body, &phy::blocks(&compiled.text))?;
    std::fs::write(&phy_path, phy::write(&compiled, &text)).map_err(|e| format!("{}: {e}", phy_path.display()))?;
    println!("dms-homer: {} (+ .phy with NM's masses, constraints and self-collision pairs)", mdl.display());

    let checked = phy::read(&std::fs::read(&phy_path).map_err(|e| e.to_string())?)?;
    report(&dms_homer::check_phy(&body, &checked)?);
    Ok(())
}

/// Homer's body from the game, as `tools/dms-game` reads it (the Playroom's way).
fn homer_body(game: &Path) -> Result<dms_game::body::Body, String> {
    let g = dms_game::Game::open(game)?;
    if let Some(m) = g.materials() {
        dms_game::set_surfaces(&m);
    }
    let fred = g.fred()?;
    // SAFETY: `fred` is `len` bytes.
    if unsafe { delirium_physics::nm_init_memory(fred.as_ptr(), fred.len() as i32) } < 0 {
        return Err("nm_init_memory: fred.xrt didn't load".into());
    }
    let (b, frames) = g.homer()?;
    Ok(dms_game::body::Body::new(&b, &frames))
}

fn report(checks: &[dms_homer::PhyCheck]) {
    println!("dms-homer: .phy against NM's body, faces as VPhysics-Box3D grows them by {:.2} mm:", dms_homer::hull::INFLATION * 1000.0);
    for c in checks {
        println!("  {:16} com {:.2} mm  faces {:+.2}/{:+.2} mm  {:3} points  volume {:+.2} %  mass {} / NM {}  unit inertia {:.6} {:.6} {:.6}",
            c.name, c.com * 1000.0, -c.face_in * 1000.0, c.face_out * 1000.0, c.points, (c.volume / c.nm_volume - 1.0) * 100.0,
            c.mass.map_or("?".into(), |m| m.to_string()), c.nm_mass, c.unit_inertia.x, c.unit_inertia.y, c.unit_inertia.z);
    }
}
