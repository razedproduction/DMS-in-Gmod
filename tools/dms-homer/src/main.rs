//! dms-homer: makes `garrysmod/models/dms/homer.mdl` (and `.phy`, materials) from the user's copy of GTA IV, the
//! body NM's own (ROADMAP M3, docs/adr/0013). The model and everything made from the game stay on this machine.

use std::path::{Path, PathBuf};
use std::process::Command;

use dms_homer::{phy, smd};

const USAGE: &str = "\
usage: dms-homer [--game <GTA IV dir>] [--gmod <GarrysMod dir>] [--work <dir>] [--joints <file> --model <dms/proxy/x.mdl>]
  makes garrysmod\\models\\dms\\homer.mdl (+ .phy, .vvd, .vtx) from the game's Homer (m_y_pmanhat_01): NM's body,
  drawn as the Playroom draws it, with studiomdl from Garry's Mod's bin\\win64, and checks the compiled .phy against NM's body.
  --game  GTA IV's folder (default: DMS_GAME_DIR, else garrysmod\\data\\dms\\game_dir.txt, else the repository's
          golden\\game_dir.txt)
  --gmod  Garry's Mod's folder (default: DMS_GMOD_DIR, else Steam's library from the registry)
  --work  where the SMDs, the .qc and studiomdl's log go (default: the repository's target\\dms-homer, else
          %TEMP%\\dms-homer; never committed)
  --joints  (ROADMAP M9, a spike) a model's 21 NM joints (DMS.FitJoints: `x y z` in metres a line): Homer's body
          fitted to them (`Build::fit`) is compiled as the proxy ragdoll --model instead of Homer";

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
    // The repository it was built in, when run there (the release's dms-homer runs on a player's machine without it).
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).map(Path::to_path_buf).filter(|r| r.join("Cargo.toml").is_file());
    let read_dir = |p: PathBuf| std::fs::read_to_string(p).ok().map(|t| PathBuf::from(t.trim())).filter(|p| !p.as_os_str().is_empty());
    let gmod = take("--gmod").or_else(|| std::env::var_os("DMS_GMOD_DIR").map(PathBuf::from)).or_else(dms_homer::steam_gmod)
        .ok_or("Garry's Mod not found: pass --gmod <GarrysMod folder>")?;
    let game = take("--game")
        .or_else(|| std::env::var_os("DMS_GAME_DIR").map(PathBuf::from))
        .or_else(|| read_dir(gmod.join("garrysmod").join("data").join("dms").join("game_dir.txt")))
        .or_else(|| repo.as_ref().and_then(|r| read_dir(r.join("golden").join("game_dir.txt"))))
        .ok_or("no game folder: pass --game <your GTA IV folder>")?;
    let joints = take("--joints");
    let model = take("--model").map(|p| p.to_string_lossy().replace('\\', "/"));
    let work = take("--work").or_else(|| repo.as_ref().map(|r| r.join("target").join("dms-homer")))
        .unwrap_or_else(|| std::env::temp_dir().join("dms-homer"));
    if !args.is_empty() {
        return Err(format!("unknown arguments {args:?}\n{USAGE}"));
    }
    let studiomdl = gmod.join("bin").join("win64").join("studiomdl.exe");
    let garrysmod = gmod.join("garrysmod");
    if !studiomdl.is_file() || !garrysmod.is_dir() {
        return Err(format!("{}: not Garry's Mod's folder (want bin\\win64\\studiomdl.exe and garrysmod\\)", gmod.display()));
    }

    println!("dms-homer: Homer's body from {}", game.display());
    let (build, frames) = homer_build(&game)?;
    let mut body = dms_game::body::Body::new(&build, &frames);
    let model = match (&joints, model) {
        (Some(j), Some(m)) => {
            body = fitted_body(&build, &frames, j)?;
            println!("dms-homer: fitted to {} as {m}", j.display());
            m
        }
        (None, None) => dms_homer::MODEL.to_string(),
        _ => return Err("--joints and --model go together".into()),
    };
    let proxy = joints.is_some();
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
    write("homer.qc", &dms_homer::qc_named(&body, &model, "homer_ref.smd", "homer_phys.smd"))?;

    println!("dms-homer: studiomdl ({})", studiomdl.display());
    let log = work.join("studiomdl.log");
    let out = Command::new(&studiomdl)
        .current_dir(&work)
        .arg("-game").arg(&garrysmod).args(["-nop4", "-verbose"]).arg(work.join("homer.qc"))
        .output().map_err(|e| format!("studiomdl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    std::fs::write(&log, &text).map_err(|e| e.to_string())?;
    let mdl = garrysmod.join("models").join(model.replace('/', "\\"));
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
    if proxy {
        report(&dms_homer::check_phy(&body, &phy::read(&std::fs::read(&phy_path).map_err(|e| e.to_string())?)?)?);
        return Ok(());
    }
    // The mark the addon checks at start (dms_init.lua): which body this model is.
    let mark = mdl.with_file_name("homer_version.txt");
    let text = format!("model {}\ndms-homer {}\n", dms_game::MODEL_VERSION, env!("CARGO_PKG_VERSION"));
    std::fs::write(&mark, text).map_err(|e| format!("{}: {e}", mark.display()))?;

    let checked = phy::read(&std::fs::read(&phy_path).map_err(|e| e.to_string())?)?;
    report(&dms_homer::check_phy(&body, &checked)?);
    Ok(())
}

/// Homer's build and standing frames from the game, as `tools/dms-game` reads them (the Playroom's way).
fn homer_build(game: &Path) -> Result<(delirium::ragdoll::Build, Vec<delirium::prelude::Transform>), String> {
    let g = dms_game::Game::open(game)?;
    if let Some(m) = g.materials() {
        dms_game::set_surfaces(&m);
    }
    let fred = g.fred()?;
    // SAFETY: `fred` is `len` bytes.
    if unsafe { delirium_physics::nm_init_memory(fred.as_ptr(), fred.len() as i32) } < 0 {
        return Err("nm_init_memory: fred.xrt didn't load".into());
    }
    g.homer()
}

/// Homer's body fitted to a model's 21 joints (`Build::fit` on his standing frames, as the module fits it).
fn fitted_body(build: &delirium::ragdoll::Build, frames: &[delirium::prelude::Transform], path: &Path) -> Result<dms_game::body::Body, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let joints: Vec<delirium::prelude::Vec3> = text
        .lines()
        .filter_map(|l| {
            let v: Vec<f32> = l.split_whitespace().filter_map(|w| w.parse().ok()).collect();
            (v.len() == 3).then(|| delirium::prelude::Vec3::new(v[0], v[1], v[2]))
        })
        .collect();
    if joints.len() != build.parts.len() {
        return Err(format!("{}: {} joints, {} wanted", path.display(), joints.len(), build.parts.len()));
    }
    let (b, f) = build.fit(frames, &joints);
    Ok(dms_game::body::Body::new(&b, &f))
}

fn report(checks: &[dms_homer::PhyCheck]) {
    println!("dms-homer: .phy against NM's body, faces as VPhysics-Box3D grows them by {:.2} mm:", dms_homer::hull::INFLATION * 1000.0);
    for c in checks {
        println!("  {:16} com {:.2} mm  faces {:+.2}/{:+.2} mm  {:3} points  volume {:+.2} %  mass {} / NM {}  unit inertia {:.6} {:.6} {:.6}",
            c.name, c.com * 1000.0, -c.face_in * 1000.0, c.face_out * 1000.0, c.points, (c.volume / c.nm_volume - 1.0) * 100.0,
            c.mass.map_or("?".into(), |m| m.to_string()), c.nm_mass, c.unit_inertia.x, c.unit_inertia.y, c.unit_inertia.z);
    }
}
