//! dms-ctl: drives the bench server (ROADMAP M2.5) from outside: the module's channel (`module/src/dev.rs`), Source
//! RCON, the Playroom's scenarios (`golden/scenarios/`, `golden/README.md`) and its `--rec` records.
//!
//! The bench's addresses and secrets come from `.bench/server.txt`, which `scripts\server.ps1 start` writes.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The repository (tools/dms-ctl -> ..).
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("the repository").to_path_buf()
}

/// Where the bench server is and how to reach it.
#[derive(Clone, Debug, Default)]
pub struct Bench {
    pub dir: String,
    pub pid: String,
    pub rcon_port: u16,
    pub rcon_password: String,
    pub dev_port: u16,
    pub dev_token: String,
    pub log: String,
}

impl Bench {
    /// `.bench/server.txt`: `key=value` lines.
    pub fn load() -> Result<Bench, String> {
        let path = repo().join(".bench").join("server.txt");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("{}: {e} (start the bench server: scripts\\server.ps1 start)", path.display()))?;
        let kv: HashMap<&str, &str> = text.trim_start_matches('\u{feff}').lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim(), v.trim())).collect();
        let get = |k: &str| kv.get(k).map(|v| v.to_string()).unwrap_or_default();
        Ok(Bench {
            dir: get("dir"),
            pid: get("pid"),
            rcon_port: get("rcon_port").parse().unwrap_or(27015),
            rcon_password: get("rcon_password"),
            dev_port: get("dev_port").parse().unwrap_or(27099),
            dev_token: get("dev_token"),
            log: get("log"),
        })
    }
}

/// A connection to the module's channel.
pub struct Channel {
    out: TcpStream,
    input: BufReader<TcpStream>,
    next: u64,
}

/// One frame from the module: `<id> <kind> <len>\n<text>`.
pub struct Frame {
    pub id: u64,
    pub kind: String,
    pub text: String,
}

/// What a read that ran out of time fails with.
const TIMEOUT: &str = "timed out";

fn io_error(e: std::io::Error) -> String {
    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) { TIMEOUT.into() } else { e.to_string() }
}

fn read_frame(r: &mut impl BufRead) -> Result<Frame, String> {
    let mut head = String::new();
    if r.read_line(&mut head).map_err(io_error)? == 0 {
        return Err("the module closed the connection".into());
    }
    let mut w = head.split_whitespace();
    let bad = || format!("a bad frame from the module: {head:?}");
    let id = w.next().and_then(|x| x.parse().ok()).ok_or_else(bad)?;
    let kind = w.next().ok_or_else(bad)?.to_string();
    let len: usize = w.next().and_then(|x| x.parse().ok()).ok_or_else(bad)?;
    let mut text = vec![0; len];
    r.read_exact(&mut text).map_err(io_error)?;
    Ok(Frame { id, kind, text: String::from_utf8_lossy(&text).into_owned() })
}

impl Channel {
    /// Connects to 127.0.0.1:`port` and authenticates.
    pub fn connect(port: u16, token: &str) -> Result<Channel, String> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let s = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
            .map_err(|e| format!("the module's channel at {addr}: {e} (dms_dev 1 on the bench server?)"))?;
        let _ = s.set_nodelay(true);
        let input = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
        let mut c = Channel { out: s, input, next: 1 };
        c.send_frame(0, "auth", token)?;
        c.set_timeout(Some(Duration::from_secs(5)));
        let f = read_frame(&mut c.input)?;
        if f.kind != "ok" {
            return Err(format!("the module refused the token: {}", f.text));
        }
        Ok(c)
    }

    /// Connects with the bench's settings.
    pub fn bench(b: &Bench) -> Result<Channel, String> {
        Channel::connect(b.dev_port, &b.dev_token)
    }

    fn set_timeout(&mut self, t: Option<Duration>) {
        let _ = self.input.get_ref().set_read_timeout(t);
    }

    fn send_frame(&mut self, id: u64, kind: &str, text: &str) -> Result<(), String> {
        let mut f = format!("{id} {kind} {}\n", text.len()).into_bytes();
        f.extend_from_slice(text.as_bytes());
        self.out.write_all(&f).map_err(|e| format!("the module's channel: {e}"))
    }

    /// Sends a request and waits up to `timeout` for its answer (telemetry frames in between are skipped).
    pub fn request(&mut self, kind: &str, text: &str, timeout: Duration) -> Result<String, String> {
        let id = self.next;
        self.next += 1;
        self.send_frame(id, kind, text)?;
        let end = Instant::now() + timeout;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("no answer to {kind} in {:.0} s", timeout.as_secs_f32()));
            }
            self.set_timeout(Some(left));
            let f = read_frame(&mut self.input).map_err(|e| {
                if e == TIMEOUT {
                    format!("no answer to {kind} in {:.0} s (is the server's Think running?)", timeout.as_secs_f32())
                } else {
                    e
                }
            })?;
            if f.id != id {
                continue;
            }
            return if f.kind == "ok" { Ok(f.text) } else { Err(f.text) };
        }
    }

    /// Asks for telemetry (`fields`: comma separated, empty for all) and hands each line to `each` until
    /// `duration` is over or `each` returns false.
    pub fn watch(&mut self, fields: &str, duration: Duration, mut each: impl FnMut(&str) -> bool) -> Result<usize, String> {
        self.request("watch", fields, Duration::from_secs(5))?;
        let end = Instant::now() + duration;
        let mut n = 0;
        while let Some(left) = end.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()) {
            self.set_timeout(Some(left));
            let f = match read_frame(&mut self.input) {
                Ok(f) => f,
                Err(e) if e == TIMEOUT => break,
                Err(e) => return Err(e),
            };
            if f.kind == "tel" {
                n += 1;
                if !each(&f.text) {
                    break;
                }
            }
        }
        let _ = self.request("unwatch", "", Duration::from_secs(5));
        Ok(n)
    }
}

/// Source RCON (TCP, little-endian `size id type body\0\0`).
pub mod rcon {
    use super::*;

    const AUTH: i32 = 3;
    const AUTH_RESPONSE: i32 = 2;
    const EXEC: i32 = 2;
    const RESPONSE: i32 = 0;

    pub struct Rcon {
        s: TcpStream,
    }

    fn packet(id: i32, kind: i32, body: &str) -> Vec<u8> {
        let mut p = Vec::with_capacity(body.len() + 14);
        p.extend_from_slice(&((body.len() + 10) as i32).to_le_bytes());
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&kind.to_le_bytes());
        p.extend_from_slice(body.as_bytes());
        p.extend_from_slice(&[0, 0]);
        p
    }

    impl Rcon {
        pub fn connect(port: u16, password: &str) -> Result<Rcon, String> {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            let s = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(|e| format!("RCON at {addr}: {e}"))?;
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let mut r = Rcon { s };
            r.s.write_all(&packet(1, AUTH, password)).map_err(|e| e.to_string())?;
            // An empty RESPONSE_VALUE comes first, then AUTH_RESPONSE with the id, or -1 for a wrong password.
            loop {
                let (id, kind, _) = r.read()?;
                if kind == AUTH_RESPONSE {
                    return if id == -1 { Err("RCON: wrong password".into()) } else { Ok(r) };
                }
            }
        }

        fn read(&mut self) -> Result<(i32, i32, String), String> {
            let mut n = [0u8; 4];
            self.s.read_exact(&mut n).map_err(|e| format!("RCON: {e}"))?;
            let n = i32::from_le_bytes(n);
            if !(10..=1 << 20).contains(&n) {
                return Err(format!("RCON: a bad packet size {n}"));
            }
            let mut b = vec![0u8; n as usize];
            self.s.read_exact(&mut b).map_err(|e| format!("RCON: {e}"))?;
            let id = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            let kind = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
            let body = &b[8..b.len().saturating_sub(2)];
            Ok((id, kind, String::from_utf8_lossy(body).into_owned()))
        }

        /// Runs a console command; its output. A long one comes in several packets: an empty RESPONSE_VALUE sent
        /// after the command comes back after all of them.
        pub fn exec(&mut self, command: &str) -> Result<String, String> {
            self.s.write_all(&packet(2, EXEC, command)).map_err(|e| e.to_string())?;
            self.s.write_all(&packet(3, RESPONSE, "")).map_err(|e| e.to_string())?;
            let mut out = String::new();
            loop {
                let (id, kind, body) = self.read()?;
                match (id, kind) {
                    (3, RESPONSE) => {
                        // srcds mirrors the empty packet, then sends one more with 0x01 00 00 00.
                        let _ = self.s.set_read_timeout(Some(Duration::from_millis(200)));
                        let _ = self.read();
                        let _ = self.s.set_read_timeout(Some(Duration::from_secs(5)));
                        return Ok(out);
                    }
                    (2, RESPONSE) => out.push_str(&body),
                    _ => {}
                }
            }
        }
    }
}

/// A Playroom scenario (`golden/README.md`): its mode and its commands, parsed as the Playroom does.
pub struct Scenario {
    pub name: String,
    /// `headless <steps> [press=a,b]` or `window`.
    pub mode: String,
    pub headless_steps: Option<u64>,
    /// `<t> <command>`, sorted by time (stable).
    pub commands: Vec<(f32, String)>,
    /// `#! tol <check> ...`: what Homer on Box3D has to keep of the Playroom's record (`golden behaviour`).
    pub tol: Vec<String>,
}

impl Scenario {
    pub fn read(path: &Path) -> Result<Scenario, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        Scenario::parse(&name, &text)
    }

    pub fn parse(name: &str, text: &str) -> Result<Scenario, String> {
        let head = text.lines().find_map(|l| l.trim().strip_prefix("#!")).ok_or("no `#! headless <steps>` or `#! window` line")?;
        let words: Vec<&str> = head.split_whitespace().collect();
        let headless_steps = match words.first().copied() {
            Some("headless") => Some(words.get(1).and_then(|n| n.parse().ok()).ok_or("#! headless <steps>")?),
            Some("window") => None,
            _ => return Err("`#! headless <steps> [press=a,b]` or `#! window`".into()),
        };
        // The Playroom's `--script`: everything after the first `#` goes, a time and a command split at the first
        // space, lines with no time dropped, sorted by time keeping the file's order.
        let mut commands: Vec<(f32, String)> = text
            .lines()
            .map(|l| l.split('#').next().unwrap_or("").trim())
            .filter(|l| !l.is_empty())
            .filter_map(|l| {
                let (t, rest) = l.split_once(' ')?;
                Some((t.parse().ok()?, rest.trim().to_string()))
            })
            .collect();
        commands.sort_by(|a, b| a.0.total_cmp(&b.0));
        if headless_steps.is_none() && !commands.iter().any(|c| c.1 == "!quit") {
            return Err("a window scenario ends with `<t> !quit`".into());
        }
        let tol = text
            .lines()
            .filter_map(|l| l.trim().strip_prefix("#!")?.trim().strip_prefix("tol ")?.split('#').next())
            .flat_map(|l| l.split_whitespace().map(String::from).collect::<Vec<_>>())
            .collect();
        Ok(Scenario { name: name.to_string(), mode: words.join(" "), headless_steps, commands, tol })
    }

    /// The module's `run` request: the mode line (and `lead` for the LEAD bench), then the commands (none for
    /// headless: the Playroom's headless takes only its buttons). A window's go as `@<step> <command>`, the NM step
    /// before which the Playroom ran each (docs/adr/0011): read off its record `theirs` (the `#` line each leaves;
    /// `!quit` after its last step), or else from the time since the first command. `!poke` takes the impulse the
    /// Playroom's record shows (its camera, which the poke comes from, moves in the window).
    pub fn request(&self, theirs: Option<&rec::Rec>, lead: bool) -> String {
        let mut s = format!("{}{}\n", self.mode, if lead { " lead" } else { "" });
        if self.headless_steps.is_some() {
            return s;
        }
        let t0 = self.commands.first().map_or(0.0, |c| c.0);
        let mut next = 0;
        for (t, c) in &self.commands {
            let estimate = ((t - t0) * 60.0).round() as i64;
            let mut step = estimate.max(0) as u64;
            let mut line = c.clone();
            if let Some(r) = theirs {
                if c == "!quit" {
                    step = r.steps.len() as u64;
                } else if let Some(sig) = mark_of(c) {
                    let found = r.log.iter().enumerate().skip(next).find(|(_, (lt, text))| {
                        text.starts_with(&sig) && ((lt * 60.0).round() as i64 - estimate).abs() <= 6
                    });
                    if let Some((i, (lt, _))) = found {
                        step = (lt * 60.0).round() as u64;
                        next = i + 1;
                        if c.starts_with("!poke") {
                            let imp = r.log[next..].iter().find_map(|(_, l)| l.strip_prefix("ours  MSG applyImpulse")?.split_whitespace().find_map(|w| w.strip_prefix("impulse=")));
                            if let (Some(imp), Some(k)) = (imp, c.split_whitespace().nth(1)) {
                                line = format!("!poke {k} {imp}");
                            }
                        }
                    }
                }
            }
            s += &format!("@{step} {line}\n");
        }
        s
    }

    /// How long it runs in real time, roughly.
    pub fn seconds(&self) -> f32 {
        match self.headless_steps {
            Some(n) => n as f32 / 60.0,
            None => self.commands.iter().map(|c| c.0).fold(0.0, f32::max),
        }
    }
}

/// The start of the `#` line the Playroom's record has for a command (`room  <button>`, `ours  MSG <message>`...).
fn mark_of(c: &str) -> Option<String> {
    let mut w = c.splitn(2, ' ');
    Some(match (w.next()?, w.next().map(str::trim)) {
        ("!button", Some("get-up")) => "ours  get-up:".into(),
        ("!button", Some("fall")) => "ours  fall:".into(),
        ("!button", Some(b)) => format!("room  {b}"),
        ("!poke", _) => "room  poke".into(),
        // The room's reactions (ROADMAP M6): the line each says first.
        ("!blast", _) => "room  blast".into(),
        ("!shoot", _) => "room  shot (".into(),
        ("!miss", _) => "room  shot: ".into(),
        ("!hfall", _) => "both  hfall".into(),
        (m, _) if m.starts_with('!') => return None,
        (m, _) => format!("ours  MSG {m}"),
    })
}

/// Runs a scenario on the bench (real time) and gives its `.rec`: on Homer in FOLLOW, or on the LEAD bench
/// (`lead`); a window's commands at the Playroom's steps when its record `theirs` is given.
pub fn run(c: &mut Channel, s: &Scenario, theirs: Option<&rec::Rec>, lead: bool) -> Result<String, String> {
    let limit = Duration::from_secs_f32(s.seconds() + 90.0);
    c.request("run", &s.request(theirs, lead), limit)
}

/// A `--rec` record, read back (`golden/README.md`).
pub mod rec {
    /// One step's line.
    #[derive(Clone, Debug)]
    pub struct Step {
        pub t: f32,
        pub com: [f32; 3],
        pub speed: f32,
        /// The two lowest parts: index, centre, contacts.
        pub low: Vec<(usize, [f32; 3], u32)>,
    }

    pub struct Rec {
        pub steps: Vec<Step>,
        /// `# <t> <text>` lines: the log.
        pub log: Vec<(f32, String)>,
    }

    fn nums<const N: usize>(w: &[&str]) -> Option<[f32; N]> {
        let v: Vec<f32> = w.iter().take(N).map(|x| x.parse().ok()).collect::<Option<_>>()?;
        v.try_into().ok()
    }

    pub fn parse(text: &str) -> Rec {
        let (mut steps, mut log) = (vec![], vec![]);
        for l in text.lines() {
            if let Some(rest) = l.strip_prefix("# ") {
                if let Some((t, s)) = rest.split_once(' ').and_then(|(t, s)| Some((t.parse().ok()?, s))) {
                    log.push((t, s.to_string()));
                }
                continue;
            }
            if !l.starts_with("- ") {
                continue;
            }
            let groups: Vec<&str> = l.split(" | ").collect();
            let Some(ours) = groups.get(1).map(|g| g.split_whitespace().collect::<Vec<_>>()) else { continue };
            let Some([t, x, y, z, v]) = nums::<5>(&ours) else { continue };
            let low = groups[2..]
                .iter()
                .filter_map(|g| {
                    let w: Vec<&str> = g.split_whitespace().collect();
                    let k = w.first()?.strip_prefix('p')?.parse().ok()?;
                    let p = nums::<3>(&w[1..])?;
                    let n = w.get(4)?.strip_prefix('n')?.parse().ok()?;
                    Some((k, p, n))
                })
                .collect();
            steps.push(Step { t, com: [x, y, z], speed: v, low });
        }
        Rec { steps, log }
    }

    /// The commands' steps: the `ours  MSG` / `room` lines' times, less the Playroom's own start-up notes
    /// (`room  Safe Mode: …`, `room  settings from …`), which aren't commands.
    pub fn marks(r: &Rec) -> Vec<(f32, String)> {
        let setup = |s: &str| s.starts_with("room  Safe Mode") || s.starts_with("room  settings from");
        r.log.iter().filter(|(_, s)| (s.starts_with("ours  MSG") || s.starts_with("room  ")) && !setup(s)).cloned().collect()
    }

    /// How `ours` differs from `theirs`, step by step from the first.
    pub struct Diff {
        pub steps: usize,
        pub theirs_steps: usize,
        /// Largest COM gap (m) and its step.
        pub com_max: (f32, usize),
        pub com_mean: f32,
        pub speed_max: f32,
        /// The first step whose COM is off by more than the tolerance.
        pub first_off: Option<usize>,
        /// Steps whose lowest part differs.
        pub low_part_mismatch: usize,
        pub marks_ours: Vec<(f32, String)>,
        pub marks_theirs: Vec<(f32, String)>,
    }

    pub fn compare(ours: &Rec, theirs: &Rec, tolerance: f32) -> Diff {
        let n = ours.steps.len().min(theirs.steps.len());
        let (mut com_max, mut sum, mut speed_max, mut first_off, mut low) = ((0.0f32, 0), 0.0f32, 0.0f32, None, 0);
        for (i, (a, b)) in ours.steps.iter().zip(&theirs.steps).enumerate() {
            let d = ((a.com[0] - b.com[0]).powi(2) + (a.com[1] - b.com[1]).powi(2) + (a.com[2] - b.com[2]).powi(2)).sqrt();
            if d > com_max.0 {
                com_max = (d, i);
            }
            if d > tolerance && first_off.is_none() {
                first_off = Some(i);
            }
            sum += d;
            speed_max = speed_max.max((a.speed - b.speed).abs());
            if a.low.first().map(|p| p.0) != b.low.first().map(|p| p.0) {
                low += 1;
            }
        }
        Diff {
            steps: ours.steps.len(),
            theirs_steps: theirs.steps.len(),
            com_max,
            com_mean: if n > 0 { sum / n as f32 } else { 0.0 },
            speed_max,
            first_off,
            low_part_mismatch: low,
            marks_ours: marks(ours),
            marks_theirs: marks(theirs),
        }
    }
}

/// `2s`, `500ms`, `1.5` (seconds), `2m`.
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let (num, scale) = if let Some(n) = s.strip_suffix("ms") {
        (n, 0.001)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60.0)
    } else {
        (s, 1.0)
    };
    let v: f64 = num.trim().parse().map_err(|_| format!("{s}: a time like 2s, 500ms, 1m"))?;
    if !(v >= 0.0 && v * scale < 86400.0) {
        return Err(format!("{s}: out of range"));
    }
    Ok(Duration::from_secs_f64(v * scale))
}
