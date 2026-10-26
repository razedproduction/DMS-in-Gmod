//! golden: the Playroom (`delirium-room`) as the reference for DMS (docs/adr/0008).
//!
//! Builds the Playroom out of the SDK's folder without writing there, dumps Homer's build, checks that DMS
//! reads the same inputs from the game (build, derived links) and records the scenarios of `golden/scenarios/`
//! into `golden/out/` (made from the user's game files: never committed, docs/adr/0006). See `golden/README.md`.

mod body;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use delirium_physics as sdk;

const USAGE: &str = "\
usage: golden [--game <GTA IV dir>] [--sdk <SDK dir>] <command>
  build        build the Playroom into target-playroom/ (the SDK's folder is checked to stay unchanged)
  dump         the Playroom's NM_DUMP of Homer -> golden/out/homer.nmdump, against DMS's own (dms-game)
  links        the idle-derived links DMS computes, fed to the Playroom (NM_LINKS): its run must not change
  lead         the SDK's LEAD agent (as dms_selftest makes it) against the Playroom's first second, bit for bit
  run [names]  the scenarios of golden/scenarios/ (all, or the named ones) -> golden/out/<name>.*
  all          build, dump, links, lead, run: every reference in one go
  body [--gmod <GarrysMod dir>] [--no-game]
               Homer's body as dms-homer ports it (ROADMAP M3): the build against the Playroom's NM_DUMP and the
               SDK (1e-5), the compiled homer.phy against the build, and on the bench (unless --no-game) Homer
               spawned and bound against NM's body (frames 1 mm / 0.5 deg, masses, inertia 1 %), then dropped
  behaviour --source [names]
               the scenarios on the bench server (dms-ctl run) -> golden/out/source/<name>.rec, against the
               Playroom's golden/out/<name>.rec: COM, speed, lowest parts, command steps
The game's folder: --game (remembered in golden/game_dir.txt), or the DMS_GAME_DIR environment variable.";

struct Ctx {
    repo: PathBuf,
    sdk: PathBuf,
    out: PathBuf,
    game: Option<PathBuf>,
}

impl Ctx {
    fn playroom(&self) -> PathBuf {
        self.repo.join("target-playroom").join("release").join("delirium-room.exe")
    }

    fn game(&self) -> Result<&Path, String> {
        self.game.as_deref().ok_or_else(|| {
            "no game folder: pass --game <your GTA IV folder> (PC: with GTAIV.exe; Xbox 360: with default.xex)".into()
        })
    }

    /// The Playroom with `--game` and `args`, run in golden/out/ (its own files land there, not in the SDK).
    fn room(&self, args: &[&str]) -> Result<Command, String> {
        let exe = self.playroom();
        if !exe.is_file() {
            return Err(format!("{}: no Playroom yet (golden build)", exe.display()));
        }
        std::fs::create_dir_all(&self.out).map_err(|e| format!("{}: {e}", self.out.display()))?;
        let mut c = Command::new(exe);
        c.current_dir(&self.out).arg("--game").arg(self.game()?).args(args);
        c.env_remove("NM_DUMP").env_remove("NM_LINKS").env_remove("NM_BUILD").env_remove("XRT").env_remove("TP");
        Ok(c)
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take = |flag: &str| {
        args.iter().position(|a| a == flag).map(|i| {
            let v = args.get(i + 1).cloned().unwrap_or_default();
            args.drain(i..(i + 2).min(args.len()));
            v
        })
    };
    // tools/golden -> the repository.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("the repository").to_path_buf();
    let sdk = take("--sdk").map(PathBuf::from).unwrap_or_else(|| {
        repo.parent().unwrap_or(&repo).join("delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)")
    });
    let remembered = repo.join("golden").join("game_dir.txt");
    let game = match take("--game") {
        Some(g) => {
            let _ = std::fs::create_dir_all(repo.join("golden")).and_then(|_| std::fs::write(&remembered, &g));
            Some(PathBuf::from(g))
        }
        None => std::env::var_os("DMS_GAME_DIR")
            .map(PathBuf::from)
            .or_else(|| std::fs::read_to_string(&remembered).ok().map(|t| PathBuf::from(t.trim()))),
    };
    let ctx = Ctx { out: repo.join("golden").join("out"), repo, sdk, game };
    let Some(cmd) = (!args.is_empty()).then(|| args.remove(0)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let r = match cmd.as_str() {
        "build" => build(&ctx),
        "dump" => dump(&ctx),
        "links" => links(&ctx),
        "lead" => lead(&ctx),
        "run" => run(&ctx, &args),
        "behaviour" => behaviour(&ctx, &args),
        "body" => body_check(&ctx, &args),
        "all" => build(&ctx).and_then(|_| dump(&ctx)).and_then(|_| links(&ctx)).and_then(|_| lead(&ctx)).and_then(|_| run(&ctx, &[])),
        _ => Err(USAGE.into()),
    };
    if let Err(e) = r {
        eprintln!("golden: {e}");
        std::process::exit(1);
    }
}

/// `git status --porcelain` of the SDK, and its Cargo.lock.
fn sdk_state(sdk: &Path) -> Result<(String, Vec<u8>), String> {
    let o = Command::new("git").arg("-C").arg(sdk).args(["status", "--porcelain", "--untracked-files=all"]).output()
        .map_err(|e| format!("git: {e}"))?;
    if !o.status.success() {
        return Err(format!("git status in {}: {}", sdk.display(), String::from_utf8_lossy(&o.stderr)));
    }
    let lock = std::fs::read(sdk.join("Cargo.lock")).map_err(|e| format!("{}: {e}", sdk.join("Cargo.lock").display()))?;
    Ok((String::from_utf8_lossy(&o.stdout).into_owned(), lock))
}

/// ADR 0008's build: `--locked`, `--target-dir` here, run from here (so the SDK's `.cargo/config.toml`, with
/// its AVX2 `target-cpu`, isn't picked up: this repository builds without AVX2).
fn build(ctx: &Ctx) -> Result<(), String> {
    let before = sdk_state(&ctx.sdk)?;
    println!("golden: building the Playroom (cargo build --release -p delirium-room --locked) into target-playroom/");
    let status = Command::new("cargo")
        .current_dir(&ctx.repo)
        .args(["build", "--release", "-p", "delirium-room", "--locked", "--manifest-path"])
        .arg(ctx.sdk.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(ctx.repo.join("target-playroom"))
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    let after = sdk_state(&ctx.sdk)?;
    if before != after {
        return Err(format!("the SDK's folder changed during the build ({}): git status there", ctx.sdk.display()));
    }
    if !status.success() {
        return Err("the Playroom didn't build".into());
    }
    println!("golden: built {}; the SDK's folder is unchanged (git status, Cargo.lock)", ctx.playroom().display());
    Ok(())
}

/// DMS's own reading of the game (as the module does it), once per process: the build's dump.
fn ours(ctx: &Ctx) -> Result<&'static str, String> {
    static OURS: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    let game = ctx.game()?.to_path_buf();
    OURS.get_or_init(|| {
        let g = dms_game::Game::open(&game)?;
        if let Some(m) = g.materials() {
            dms_game::set_surfaces(&m);
        }
        let fred = g.fred()?;
        // SAFETY: `fred` is `len` bytes.
        if unsafe { sdk::nm_init_memory(fred.as_ptr(), fred.len() as i32) } < 0 {
            return Err("nm_init_memory: fred.xrt didn't load".into());
        }
        let (b, frames) = g.homer()?;
        Ok(b.dump(&frames))
    })
    .as_ref()
    .map(String::as_str)
    .map_err(Clone::clone)
}

fn write(path: &Path, data: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Runs `c`, its stdout into `stdout` (if given), stderr into `log`; kills it after `limit`.
fn run_to(mut c: Command, stdout: Option<&Path>, log: &Path, limit: Duration) -> Result<(), String> {
    let file = |p: &Path| std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()));
    c.stdin(Stdio::null()).stderr(file(log)?);
    c.stdout(match stdout {
        Some(p) => Stdio::from(file(p)?),
        None => Stdio::null(),
    });
    let mut child = c.spawn().map_err(|e| format!("the Playroom: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            return if s.success() { Ok(()) } else { Err(format!("the Playroom failed ({s}): see {}", log.display())) };
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            return Err(format!("the Playroom ran over {} s and was stopped: see {}", limit.as_secs(), log.display()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn dump(ctx: &Ctx) -> Result<(), String> {
    let theirs = ctx.out.join("homer.nmdump");
    let mut c = ctx.room(&["--headless", "0"])?;
    c.env("NM_DUMP", &theirs);
    run_to(c, None, &ctx.out.join("homer.nmdump.log"), Duration::from_secs(300))?;
    let t = read_text(&theirs)?;
    let o = ours(ctx)?;
    write(&ctx.out.join("homer.ours.nmdump"), o)?;
    println!("golden: NM_DUMP -> {} ({} parts)", theirs.display(), t.lines().count());
    if t == o {
        println!("golden: dump OK: DMS's build of Homer is the Playroom's, byte for byte");
        return Ok(());
    }
    let (k, (a, b)) = t.lines().zip(o.lines()).enumerate().find(|(_, (a, b))| a != b).unwrap_or((0, ("", "")));
    Err(format!("dump: DMS's build differs from the Playroom's (part {k}):\n  playroom {a}\n  dms      {b}"))
}

fn links(ctx: &Ctx) -> Result<(), String> {
    ours(ctx)?;
    let l = delirium::nmdbm::DERIVED_LINKS[dms_game::HOMER_NM].get().ok_or("DMS derived no links from the idle clip")?;
    let file = ctx.out.join("homer.links");
    write(&file, l.iter().map(|q| format!("{} {} {} {}\n", q.x, q.y, q.z, q.w)).collect::<String>())?;
    let base = ctx.out.join("links.playroom.frames");
    run_to(ctx.room(&["--headless", "60"])?, Some(&base), &ctx.out.join("links.playroom.log"), Duration::from_secs(300))?;
    let fed = ctx.out.join("links.dms.frames");
    let mut c = ctx.room(&["--headless", "60"])?;
    c.env("NM_LINKS", &file);
    run_to(c, Some(&fed), &ctx.out.join("links.dms.log"), Duration::from_secs(300))?;
    let (a, b) = (read_text(&base)?, read_text(&fed)?);
    if a.is_empty() || a != b {
        return Err(format!("links: the Playroom moves differently with DMS's links ({} vs {})", base.display(), fed.display()));
    }
    println!("golden: links OK: {} links; the Playroom's first second is bit for bit the same with DMS's", l.len());
    Ok(())
}

/// One step's frames as the Playroom's headless prints them: 7 floats' bits per part.
fn frame_line(t: &[f32]) -> String {
    t.chunks(7).map(|p| p.iter().map(|x| format!("{:08x}", x.to_bits())).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join(" | ")
}

fn bits(line: &str) -> Vec<f32> {
    line.split([' ', '|']).filter(|w| !w.is_empty()).filter_map(|w| u32::from_str_radix(w, 16).ok()).map(f32::from_bits).collect()
}

fn lead(ctx: &Ctx) -> Result<(), String> {
    let base = ctx.out.join("links.playroom.frames");
    if !base.is_file() {
        run_to(ctx.room(&["--headless", "60"])?, Some(&base), &ctx.out.join("links.playroom.log"), Duration::from_secs(300))?;
    }
    let theirs: Vec<Vec<f32>> = read_text(&base)?.lines().map(bits).collect();
    let dump = std::ffi::CString::new(ours(ctx)?).map_err(|e| e.to_string())?;
    let g = dms_game::Game::open(ctx.game()?)?;
    let floor_row = g.materials().map_or(0, |m| dms_game::concrete(&dms_game::surfaces(&m)));
    // SAFETY: `ours` loaded fred.xrt.
    let mine = unsafe { lead_run(&dump, floor_row, theirs.len()) }?;
    write(&ctx.out.join("lead.dms.frames"), mine.iter().map(|f| frame_line(f) + "\n").collect::<String>())?;
    let same = theirs.iter().zip(&mine).take_while(|(a, b)| a == b).count();
    let worst = theirs.iter().zip(&mine).map(|(a, b)| {
        a.chunks(7).zip(b.chunks(7)).map(|(p, q)| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()).fold(0.0, f32::max)
    }).fold(0.0, f32::max);
    if same == theirs.len() && mine.len() == theirs.len() {
        println!("golden: lead OK: DMS's agent (as dms_selftest makes it) moves as the Playroom's, bit for bit, {same} steps");
        return Ok(());
    }
    Err(format!("lead: DMS's agent matches the Playroom bit for bit for {same} of {} steps only; worst part position gap \
                 {worst:.6} m (lead.dms.frames against links.playroom.frames)", theirs.len()))
}

/// The SDK's LEAD agent as `dms_selftest` makes it, `steps` steps of 1/60 s after `bodyBalance`.
///
/// # Safety
/// fred.xrt is loaded (`ours`).
unsafe fn lead_run(dump: &std::ffi::CStr, floor_row: u8, steps: usize) -> Result<Vec<Vec<f32>>, String> {
    let (p, t) = dms_game::floor(true, 50.0);
    // SAFETY: the SDK's own calls on the build, world and agent made here, with arrays as long as they say.
    unsafe {
        let b = sdk::nm_build_from_text(dump.as_ptr());
        if b.is_null() {
            return Err("nm_build_from_text failed".into());
        }
        let w = sdk::nm_world_create();
        sdk::nm_world_add_mesh(w, p.as_ptr().cast(), p.len() as i32, t.as_ptr().cast(), t.len() as i32, vec![floor_row; t.len()].as_ptr());
        let a = sdk::nm_agent_create(b, [0.0f32; 3].as_ptr(), 0.0, 0.3);
        sdk::nm_agent_set_mode(a, sdk::NM_MODE_LEAD);
        sdk::nm_msg_begin(a, c"bodyBalance".as_ptr());
        sdk::nm_msg_set_bool(a, c"start".as_ptr(), 1);
        sdk::nm_msg_send(a, w);
        let mut out = vec![];
        for _ in 0..steps {
            sdk::nm_agent_step(a, w, 1.0 / 60.0);
            let mut f = vec![0.0f32; 21 * 7];
            sdk::nm_agent_get_part_transforms(a, f.as_mut_ptr(), 21);
            out.push(f);
        }
        sdk::nm_agent_destroy(a);
        sdk::nm_world_destroy(w);
        sdk::nm_build_destroy(b);
        Ok(out)
    }
}

/// A scenario: how the Playroom runs it.
enum Mode {
    /// `--headless <steps> [--press ..]`: deterministic, bit for bit.
    Headless { steps: usize, press: String },
    /// The window with `--script`: commands at wall-clock times.
    Window { last: f32 },
}

fn scenario(text: &str) -> Result<Mode, String> {
    let head = text.lines().find_map(|l| l.trim().strip_prefix("#!")).ok_or("no `#! headless <steps>` or `#! window` line")?;
    let w: Vec<&str> = head.split_whitespace().collect();
    match w.first().copied() {
        Some("headless") => {
            let steps = w.get(1).and_then(|n| n.parse().ok()).ok_or("#! headless <steps>")?;
            let press = w.iter().find_map(|x| x.strip_prefix("press=")).unwrap_or("").to_string();
            Ok(Mode::Headless { steps, press })
        }
        Some("window") => {
            let times: Vec<f32> = text.lines().map(|l| l.split('#').next().unwrap_or("").trim()).filter(|l| !l.is_empty())
                .filter_map(|l| l.split_whitespace().next()?.parse().ok()).collect();
            if !text.lines().any(|l| l.split('#').next().unwrap_or("").contains("!quit")) {
                return Err("a window scenario ends with `<t> !quit`".into());
            }
            Ok(Mode::Window { last: times.iter().copied().fold(0.0, f32::max) })
        }
        _ => Err("`#! headless <steps> [press=a,b]` or `#! window`".into()),
    }
}

/// The scenarios of golden/scenarios/: all, or the named ones.
fn scenarios(ctx: &Ctx, names: &[String]) -> Result<Vec<PathBuf>, String> {
    let dir = ctx.repo.join("golden").join("scenarios");
    let mut all: Vec<PathBuf> = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "txt")).collect();
    all.sort();
    if !names.is_empty() {
        all.retain(|p| names.iter().any(|n| p.file_stem().is_some_and(|s| s == n.as_str())));
        if all.len() != names.len() {
            return Err(format!("no such scenario among {}", dir.display()));
        }
    }
    Ok(all)
}

/// A COM gap (m) the record's three decimals can't tell from none.
const REC_TOLERANCE: f32 = 0.0015;

/// ROADMAP M2.5: each scenario on the bench server, recorded as the Playroom records it, against the Playroom's.
/// The bench's agent is in LEAD on NM's own floor (as `golden lead`), so this checks the bench and its records;
/// Homer's FOLLOW agent on Box3D takes its place in M4.
fn behaviour(ctx: &Ctx, args: &[String]) -> Result<(), String> {
    let mut names: Vec<String> = args.to_vec();
    let Some(i) = names.iter().position(|a| a == "--source") else {
        return Err("behaviour --source [names]: only the bench server (Source) is there to compare so far".into());
    };
    names.remove(i);
    let bench = dms_ctl::Bench::load()?;
    let mut channel = dms_ctl::Channel::bench(&bench)?;
    let out = ctx.out.join("source");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut failed = vec![];
    for path in scenarios(ctx, &names)? {
        let s = dms_ctl::Scenario::read(&path)?;
        let theirs = ctx.out.join(format!("{}.rec", s.name));
        if !theirs.is_file() {
            println!("golden: {}: no reference {} (golden run {})", s.name, theirs.display(), s.name);
            failed.push(s.name);
            continue;
        }
        println!("golden: {}: on the bench ({}, about {:.0} s)...", s.name, s.mode, s.seconds());
        let rec = match dms_ctl::run(&mut channel, &s) {
            Ok(r) => r,
            Err(e) => {
                println!("golden: {}: FAILED on the bench: {e}", s.name);
                failed.push(s.name);
                continue;
            }
        };
        write(&out.join(format!("{}.rec", s.name)), &rec)?;
        let (ours, theirs) = (dms_ctl::rec::parse(&rec), dms_ctl::rec::parse(&read_text(&theirs)?));
        let d = dms_ctl::rec::compare(&ours, &theirs, REC_TOLERANCE);
        let same = d.steps == d.theirs_steps && d.first_off.is_none() && d.low_part_mismatch == 0;
        println!("golden: {}: {} steps (Playroom {}); COM gap max {:.4} m at step {}, mean {:.4} m; |v| gap max {:.3} m/s; \
                  lowest part differs on {} steps; first step off by > {REC_TOLERANCE} m: {}",
            s.name, d.steps, d.theirs_steps, d.com_max.0, d.com_max.1 + 1, d.com_mean, d.speed_max, d.low_part_mismatch,
            d.first_off.map_or("none".into(), |k| (k + 1).to_string()));
        let marks = |m: &[(f32, String)]| m.iter().map(|(t, s)| format!("{t:.2} {s}")).collect::<Vec<_>>();
        let (mo, mt) = (marks(&d.marks_ours), marks(&d.marks_theirs));
        if mo != mt {
            println!("golden: {}: command steps differ:\n  bench    {}\n  playroom {}", s.name, mo.join(" | "), mt.join(" | "));
        }
        for l in rec.lines().filter(|l| l.starts_with("# dms:")) {
            println!("golden: {}: {l}", s.name);
        }
        if same {
            println!("golden: {}: OK, the bench's record is the Playroom's to its 3 decimals", s.name);
        } else {
            failed.push(s.name);
        }
    }
    if failed.is_empty() { Ok(()) } else { Err(format!("differ from the Playroom: {}", failed.join(", "))) }
}

fn run(ctx: &Ctx, names: &[String]) -> Result<(), String> {
    let all = scenarios(ctx, names)?;
    let mut failed = vec![];
    for path in &all {
        let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        match run_one(ctx, path, &name) {
            Ok(s) => println!("golden: {name}: {s}"),
            Err(e) => {
                println!("golden: {name}: FAILED: {e}");
                failed.push(name);
            }
        }
    }
    if failed.is_empty() { Ok(()) } else { Err(format!("scenarios failed: {}", failed.join(", "))) }
}

fn run_one(ctx: &Ctx, path: &Path, name: &str) -> Result<String, String> {
    let mode = scenario(&read_text(path)?)?;
    let rec = ctx.out.join(format!("{name}.rec"));
    let log = ctx.out.join(format!("{name}.log"));
    let rec_s = rec.to_string_lossy().into_owned();
    match &mode {
        Mode::Headless { steps, press } => {
            let n = steps.to_string();
            let mut a = vec!["--headless", n.as_str(), "--rec", rec_s.as_str()];
            if !press.is_empty() {
                a.extend(["--press", press.as_str()]);
            }
            let mut c = ctx.room(&a)?;
            c.env("ROOM_LOG", "1");
            run_to(c, Some(&ctx.out.join(format!("{name}.frames"))), &log, Duration::from_secs(600))?;
        }
        Mode::Window { last } => {
            let script = path.canonicalize().map_err(|e| e.to_string())?;
            let s = script.to_string_lossy().into_owned();
            // Safe Mode: the Playroom's defaults, not the user's settings.txt (its behaviour switches too, read by
            // the window since 0.2.1), and the lowest graphics, so the window keeps up with 60 steps a second.
            let mut c = ctx.room(&["--script", s.as_str(), "--rec", rec_s.as_str(), "--safe"])?;
            c.env("ROOM_LOG", "1");
            run_to(c, None, &log, Duration::from_secs_f32(last + 120.0))?;
        }
    }
    let text = read_text(&rec)?;
    let frames: Vec<&str> = text.lines().filter(|l| l.starts_with("- ")).collect();
    let t_end = frames.last().and_then(|l| l.split('|').nth(1)).and_then(|s| s.split_whitespace().next()).unwrap_or("0");
    let kind = match mode {
        Mode::Headless { .. } => "headless, bit for bit (.frames)",
        Mode::Window { .. } => "window: command times in .rec's # lines",
    };
    Ok(format!("{} steps to t = {t_end} s ({kind}) -> {}", frames.len(), rec.display()))
}

/// `golden body`: the three steps of `body.rs`; the game's only if the bench is up (or `--no-game`).
fn body_check(ctx: &Ctx, args: &[String]) -> Result<(), String> {
    let gmod = args.iter().position(|a| a == "--gmod").and_then(|i| args.get(i + 1)).map(PathBuf::from)
        .or_else(|| std::env::var_os("DMS_GMOD_DIR").map(PathBuf::from)).or_else(dms_homer::steam_gmod)
        .ok_or("Garry's Mod not found: pass --gmod <GarrysMod folder>")?;
    let theirs = ctx.out.join("homer.nmdump");
    if !theirs.is_file() {
        dump(ctx)?;
    }
    let ours_dump = ours(ctx)?;
    let (b, frames) = dms_game::Game::open(ctx.game()?)?.homer()?;
    let body = dms_game::body::Body::new(&b, &frames);
    let mut failed = vec![];
    if !body::build(&body, ours_dump, &read_text(&theirs)?)? {
        failed.push("build");
    }
    let phy = gmod.join("garrysmod").join("models").join(dms_homer::MODEL.replace('/', "\\")).with_extension("phy");
    if !body::phy(&body, &phy)? {
        failed.push(".phy");
    }
    if args.iter().any(|a| a == "--no-game") {
        println!("golden: body 3/3: skipped (--no-game)");
    } else if !body::game(&body)? {
        failed.push("game");
    }
    if failed.is_empty() {
        println!("golden: body OK");
        Ok(())
    } else {
        Err(format!("body: off in {}", failed.join(", ")))
    }
}
