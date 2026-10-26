//! dms-map: the stages map (ROADMAP M5). The Playroom's stages (`--stage`, `dms_game::stage`) built as brushes on one
//! flat floor, a site each along X, plus a slope that isn't the Playroom's; compiled with the game's own `vbsp`, `vvis`
//! and `vrad` into `garrysmod/maps/dms_stages.bsp`. NM's world on it comes from the `.bsp` as on any map (M5), so the
//! golden scenarios on the stages check both the map's world and FOLLOW against the Playroom's (`golden/README.md`).
//!
//! `dms-map [--gmod <GarrysMod folder>] [--no-compile]`: the `.vmf` and the build in `target/dms-map/`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use dms_game::stage::{self, Block};

const MAP: &str = "dms_stages";
const UNITS_PER_METRE: f32 = 1.0 / 0.0254;
/// The sites' spacing along X (m), and the floor's and sky's extent (m).
const SPACING: f32 = 40.0;
const FLOOR_HALF_Y: f32 = 30.0;
const SKY_TOP: f32 = 25.0;
const WALL: f32 = 0.5;
/// The materials: Source's concrete and brick (their `$surfaceprop` picks NM's CONCRETE and BRICK_WALL rows, as the
/// Playroom's boxes have them), and the sky.
const CONCRETE: &str = "CONCRETE/CONCRETEFLOOR001A";
const BRICK: &str = "BRICK/BRICKWALL001A";
const SKY: &str = "TOOLS/TOOLSSKYBOX";
/// The slope beside the stages (not the Playroom's): rise over its length, how long, how wide (m).
const SLOPE: (f32, f32, f32) = (20.0, 6.0, 4.0);

type V = [f32; 3];

/// A plane through `p` facing out along `n`, as a `.vmf` writes it: three points, `(p0 - p1) x (p2 - p1)` along `n`.
fn plane(p: V, n: V) -> String {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let n = [n[0] / len, n[1] / len, n[2] / len];
    let a = if n[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let cross = |a: V, b: V| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let u = cross(a, n);
    let v = cross(n, u);
    let s = 64.0;
    let pt = |o: V| {
        let q = [(p[0] + o[0] * s) * UNITS_PER_METRE, (p[1] + o[1] * s) * UNITS_PER_METRE, (p[2] + o[2] * s) * UNITS_PER_METRE];
        format!("({:.4} {:.4} {:.4})", q[0], q[1], q[2])
    };
    format!("{} {} {}", pt(u), pt([0.0; 3]), pt(v))
}

struct Vmf {
    text: String,
    id: u32,
}

impl Vmf {
    fn id(&mut self) -> u32 {
        self.id += 1;
        self.id
    }

    /// A convex brush of planes (point, outward normal), all sides of one material.
    fn brush(&mut self, planes: &[(V, V)], material: &str) {
        let id = self.id();
        let mut s = format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n");
        for &(p, n) in planes {
            let side = self.id();
            // Texture axes: world-aligned, the axis least along the normal first.
            let (u, v) = if n[2].abs() > 0.7 { ("[1 0 0 0] 0.25", "[0 -1 0 0] 0.25") } else if n[0].abs() > n[1].abs() { ("[0 1 0 0] 0.25", "[0 0 -1 0] 0.25") } else { ("[1 0 0 0] 0.25", "[0 0 -1 0] 0.25") };
            let _ = write!(
                s,
                "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{side}\"\n\t\t\t\"plane\" \"{}\"\n\t\t\t\"material\" \"{material}\"\n\t\t\t\"uaxis\" \"{u}\"\n\t\t\t\"vaxis\" \"{v}\"\n\t\t\t\"rotation\" \"0\"\n\t\t\t\"lightmapscale\" \"16\"\n\t\t\t\"smoothing_groups\" \"0\"\n\t\t}}\n",
                plane(p, n)
            );
        }
        s += "\t}\n";
        self.text += &s;
    }

    /// An axis-aligned box: centre and half sizes (m).
    fn cuboid(&mut self, c: V, h: V, material: &str) {
        let planes: Vec<(V, V)> = (0..3)
            .flat_map(|k| {
                [-1.0f32, 1.0].map(|s| {
                    let mut p = c;
                    p[k] += s * h[k];
                    let mut n = [0.0; 3];
                    n[k] = s;
                    (p, n)
                })
            })
            .collect();
        self.brush(&planes, material);
    }
}

fn entity(id: u32, kv: &[(&str, String)]) -> String {
    let mut s = format!("entity\n{{\n\t\"id\" \"{id}\"\n");
    for (k, v) in kv {
        let _ = writeln!(s, "\t\"{k}\" \"{v}\"");
    }
    s + "}\n"
}

fn origin(p: V) -> String {
    format!("{:.3} {:.3} {:.3}", p[0] * UNITS_PER_METRE, p[1] * UNITS_PER_METRE, p[2] * UNITS_PER_METRE)
}

/// The sites: each stage's centre on the floor (m).
fn sites() -> Vec<(&'static str, V)> {
    let names: Vec<&str> = stage::NAMES.iter().copied().chain(["slope"]).collect();
    let n = names.len() as f32;
    names.iter().enumerate().map(|(i, &s)| (s, [(i as f32 - (n - 1.0) / 2.0) * SPACING, 0.0, 0.0])).collect()
}

fn vmf() -> String {
    let mut w = Vmf { text: String::new(), id: 1 };
    let half_x = sites().len() as f32 * SPACING / 2.0 + 10.0;
    // The floor, its top at z 0, and the sky's box around it all.
    w.cuboid([0.0, 0.0, -0.5], [half_x, FLOOR_HALF_Y, 0.5], CONCRETE);
    let (hx, hy, top) = (half_x + WALL, FLOOR_HALF_Y + WALL, SKY_TOP);
    w.cuboid([0.0, 0.0, -1.0 - WALL / 2.0], [hx + WALL, hy + WALL, WALL / 2.0], SKY);
    w.cuboid([0.0, 0.0, top + WALL / 2.0], [hx + WALL, hy + WALL, WALL / 2.0], SKY);
    let mid = (top - 1.0) / 2.0;
    let h = (top + 1.0) / 2.0;
    w.cuboid([-hx, 0.0, mid], [WALL / 2.0, hy + WALL, h], SKY);
    w.cuboid([hx, 0.0, mid], [WALL / 2.0, hy + WALL, h], SKY);
    w.cuboid([0.0, -hy, mid], [hx, WALL / 2.0, h], SKY);
    w.cuboid([0.0, hy, mid], [hx, WALL / 2.0, h], SKY);
    let mut ents = String::new();
    let mut eid = 100_000;
    for (name, at) in sites() {
        if let Some(s) = stage::stage(name) {
            for (c, h, b) in s.boxes {
                w.cuboid([c[0] + at[0], c[1] + at[1], c[2] + at[2]], h, if b == Block::Brick { BRICK } else { CONCRETE });
            }
        } else {
            // The slope: a wedge rising along +Y from the site's centre.
            let (deg, len, wide) = SLOPE;
            let rise = len * deg.to_radians().tan();
            let (x, y) = (at[0], at[1]);
            w.brush(
                &[
                    ([x, y, 0.0], [0.0, 0.0, -1.0]),
                    ([x, y + len, 0.0], [0.0, 1.0, 0.0]),
                    ([x - wide / 2.0, y, 0.0], [-1.0, 0.0, 0.0]),
                    ([x + wide / 2.0, y, 0.0], [1.0, 0.0, 0.0]),
                    ([x, y, 0.0], [0.0, -rise, len]),
                ],
                CONCRETE,
            );
        }
        eid += 1;
        ents += &entity(eid, &[("classname", "info_target".into()), ("targetname", format!("dms_stage_{name}")), ("origin", origin([at[0], at[1], 0.5]))]);
    }
    let open = sites().iter().find(|s| s.0 == "open").map_or([0.0; 3], |s| s.1);
    eid += 1;
    ents += &entity(eid, &[("classname", "info_player_start".into()), ("origin", origin([open[0], open[1] - 4.0, 0.1])), ("angles", "0 90 0".into())]);
    eid += 1;
    ents += &entity(
        eid,
        &[
            ("classname", "light_environment".into()),
            ("origin", origin([0.0, 0.0, 10.0])),
            ("angles", "-50 30 0".into()),
            ("pitch", "-50".into()),
            ("_light", "255 250 235 300".into()),
            ("_ambient", "170 185 210 120".into()),
        ],
    );
    format!(
        "versioninfo\n{{\n\t\"editorversion\" \"400\"\n\t\"editorbuild\" \"8000\"\n\t\"mapversion\" \"1\"\n\t\"formatversion\" \"100\"\n\t\"prefab\" \"0\"\n}}\nworld\n{{\n\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n{}}}\n{ents}",
        w.text
    )
}

fn run(exe: &Path, args: &[&str], dir: &Path) -> Result<(), String> {
    println!("dms-map: {} {}", exe.display(), args.join(" "));
    let s = Command::new(exe).args(args).current_dir(dir).status().map_err(|e| format!("{}: {e}", exe.display()))?;
    if s.success() { Ok(()) } else { Err(format!("{} failed ({s})", exe.display())) }
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("dms-map: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gmod = args
        .iter()
        .position(|a| a == "--gmod")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("DMS_GMOD_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(r"E:\Games\Steam\steamapps\common\GarrysMod"));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let out = repo.join("target").join("dms-map");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let vmf_path = out.join(format!("{MAP}.vmf"));
    std::fs::write(&vmf_path, vmf()).map_err(|e| format!("{}: {e}", vmf_path.display()))?;
    println!("dms-map: {}", vmf_path.display());
    for (name, at) in sites() {
        println!("dms-map:   {name:<10} at x {:>6.1} m (info_target dms_stage_{name})", at[0]);
    }
    if args.iter().any(|a| a == "--no-compile") {
        return Ok(());
    }
    let bin = gmod.join("bin").join("win64");
    let game = gmod.join("garrysmod");
    let g = game.to_string_lossy().into_owned();
    run(&bin.join("vbsp.exe"), &["-game", &g, MAP], &out)?;
    run(&bin.join("vvis.exe"), &["-fast", "-game", &g, MAP], &out)?;
    run(&bin.join("vrad.exe"), &["-fast", "-game", &g, MAP], &out)?;
    let bsp = out.join(format!("{MAP}.bsp"));
    let to = game.join("maps").join(format!("{MAP}.bsp"));
    std::fs::copy(&bsp, &to).map_err(|e| format!("{} -> {}: {e}", bsp.display(), to.display()))?;
    println!("dms-map: {} -> {}", bsp.display(), to.display());
    Ok(())
}
