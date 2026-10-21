//! The control and telemetry channel of `dms-ctl` (ROADMAP M2.5): a TCP server on 127.0.0.1, on only while
//! `dms_dev 1` (`addon/lua/dms/dev.lua`), behind a token given on each start.
//!
//! Its threads touch neither Lua nor the physics (docs/adr/0003): they queue requests and send what the main thread
//! hands them. The main thread takes the queue in `Think` (`DMS.DevThink`): console commands and Lua go back to Lua,
//! NM messages and scenarios go to the bench agent (`bench.rs`), and each frame's telemetry is published.
//!
//! Framing, both ways: a header line, then `len` bytes of UTF-8.
//!   client: `<id> <kind> <len>\n<text>`; the first request is `0 auth <len>\n<token>`
//!   server: `<id> ok|err <len>\n<text>` answers request `id`; `0 tel <len>\n<json>` is telemetry (after `watch`)
//! Kinds: `auth`, `ping`, `snapshot [fields]`, `watch [fields]`, `unwatch` (answered here); `exec`, `lua`, `nm`,
//! `run`, `bench` (in `Think`).

use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, c_int};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use crate::bench::{Bench, ScriptMode, json};
use crate::data;
use crate::lua::{self, Api, LuaState, native};
use crate::step;

/// A request's text is at most this long (a scenario, a chunk of Lua).
const MAX_TEXT: usize = 1 << 20;
/// Frames queued for a slow client before telemetry is dropped for it.
const QUEUE: usize = 4096;

/// A request for the main thread.
struct Request {
    conn: u64,
    id: u64,
    kind: String,
    text: String,
}

struct Shared {
    stop: AtomicBool,
    token: String,
    requests: Mutex<VecDeque<Request>>,
    /// Each connection's outgoing frames, and its socket (to shut it on stop).
    conns: Mutex<HashMap<u64, (SyncSender<Vec<u8>>, TcpStream)>>,
    /// Connections watching telemetry, with the fields they asked for (all if none).
    watchers: Mutex<HashMap<u64, Option<Vec<String>>>>,
    /// The last telemetry, field by field.
    snapshot: Mutex<Vec<(&'static str, String)>>,
}

struct Server {
    shared: Arc<Shared>,
    port: u16,
    thread: Option<thread::JoinHandle<()>>,
}

/// The main thread's side: the bench and the request whose scenario runs on it.
struct Main {
    bench: Option<Bench>,
    run: Option<(u64, u64)>,
}

// SAFETY: the bench's NM objects are only used on the main thread, from `Think`.
unsafe impl Send for Main {}

static SERVER: Mutex<Option<Server>> = Mutex::new(None);
static MAIN: Mutex<Main> = Mutex::new(Main { bench: None, run: None });
static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn frame(id: u64, kind: &str, text: &str) -> Vec<u8> {
    let mut f = format!("{id} {kind} {}\n", text.len()).into_bytes();
    f.extend_from_slice(text.as_bytes());
    f
}

impl Shared {
    /// Queues `f` for connection `conn`; false if it's gone or too far behind.
    fn send(&self, conn: u64, f: Vec<u8>) -> bool {
        let conns = lock(&self.conns);
        let Some((tx, _)) = conns.get(&conn) else { return false };
        !matches!(tx.try_send(f), Err(TrySendError::Disconnected(_)))
    }

    fn reply(&self, conn: u64, id: u64, r: Result<String, String>) {
        let f = match &r {
            Ok(t) => frame(id, "ok", t),
            Err(e) => frame(id, "err", e),
        };
        self.send(conn, f);
    }
}

/// Starts listening on 127.0.0.1:`port`; requests must open with `token`. Restarts a running server.
pub fn start(port: u16, token: &str) -> Result<(), String> {
    if token.len() < 8 {
        return Err("the token must be 8 characters or more".into());
    }
    stop();
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("127.0.0.1:{port}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let shared = Arc::new(Shared {
        stop: AtomicBool::new(false),
        token: token.to_string(),
        requests: Mutex::new(VecDeque::new()),
        conns: Mutex::new(HashMap::new()),
        watchers: Mutex::new(HashMap::new()),
        snapshot: Mutex::new(vec![]),
    });
    let s = shared.clone();
    let thread = thread::Builder::new()
        .name("dms-dev".into())
        .spawn(move || accept(listener, s))
        .map_err(|e| e.to_string())?;
    *lock(&SERVER) = Some(Server { shared, port, thread: Some(thread) });
    Ok(())
}

/// Stops the server and closes every connection; drops the bench.
pub fn stop() {
    let server = lock(&SERVER).take();
    if let Some(mut s) = server {
        s.shared.stop.store(true, Ordering::SeqCst);
        for (_, (_, stream)) in lock(&s.shared.conns).drain() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(t) = s.thread.take() {
            let _ = t.join();
        }
    }
    let mut main = lock(&MAIN);
    (main.bench, main.run) = (None, None);
}

fn running() -> Option<(Arc<Shared>, u16)> {
    lock(&SERVER).as_ref().map(|s| (s.shared.clone(), s.port))
}

fn accept(listener: TcpListener, shared: Arc<Shared>) {
    while !shared.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let s = shared.clone();
                let _ = thread::Builder::new().name("dms-dev-conn".into()).spawn(move || serve(stream, s));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(50)),
            Err(_) => thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// Reads one `<id> <kind> <len>\n<text>`; None at the end of the stream or on a malformed one.
fn read_request(r: &mut impl BufRead) -> Option<(u64, String, String)> {
    let mut head = String::new();
    if r.read_line(&mut head).ok()? == 0 {
        return None;
    }
    let mut w = head.split_whitespace();
    let (id, kind, len) = (w.next()?.parse().ok()?, w.next()?.to_string(), w.next()?.parse::<usize>().ok()?);
    if len > MAX_TEXT {
        return None;
    }
    let mut text = vec![0; len];
    r.read_exact(&mut text).ok()?;
    Some((id, kind, String::from_utf8_lossy(&text).into_owned()))
}

fn serve(stream: TcpStream, shared: Arc<Shared>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let (Ok(mut out), Ok(keep)) = (stream.try_clone(), stream.try_clone()) else { return };
    let mut input = BufReader::new(stream);
    // The token comes first, or the connection goes.
    let _ = input.get_ref().set_read_timeout(Some(Duration::from_secs(5)));
    match read_request(&mut input) {
        Some((id, kind, token)) if kind == "auth" && token == shared.token => {
            let _ = out.write_all(&frame(id, "ok", "dms"));
        }
        _ => {
            let _ = out.write_all(&frame(0, "err", "auth first: 0 auth <len>\\n<token>"));
            return;
        }
    }
    let _ = input.get_ref().set_read_timeout(None);
    let conn = NEXT_CONN.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(QUEUE);
    lock(&shared.conns).insert(conn, (tx, keep));
    let writer = thread::spawn(move || {
        for f in rx {
            if out.write_all(&f).is_err() {
                break;
            }
        }
    });
    while let Some((id, kind, text)) = read_request(&mut input) {
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
        let fields = || {
            let f: Vec<String> = text.split([',', ' ']).filter(|f| !f.is_empty()).map(String::from).collect();
            (!f.is_empty()).then_some(f)
        };
        match kind.as_str() {
            "ping" => shared.reply(conn, id, Ok("pong".into())),
            "snapshot" => {
                let snap = lock(&shared.snapshot);
                let r = if snap.is_empty() {
                    Err("no telemetry yet (the server's Think hasn't run with dms_dev 1)".into())
                } else {
                    Ok(json::object(&snap, fields().as_deref()))
                };
                drop(snap);
                shared.reply(conn, id, r);
            }
            "watch" => {
                lock(&shared.watchers).insert(conn, fields());
                shared.reply(conn, id, Ok("watching".into()));
            }
            "unwatch" => {
                lock(&shared.watchers).remove(&conn);
                shared.reply(conn, id, Ok("unwatched".into()));
            }
            "exec" | "lua" | "nm" | "run" | "bench" => lock(&shared.requests).push_back(Request { conn, id, kind, text }),
            _ => shared.reply(conn, id, Err(format!("unknown request {kind}"))),
        }
    }
    lock(&shared.watchers).remove(&conn);
    // Dropping the sender ends the writer.
    lock(&shared.conns).remove(&conn);
    let _ = writer.join();
}

/// A scenario request: its `#!` mode line, then `<t> <command>` lines (`dms-ctl run` parsed the file).
fn parse_run(text: &str) -> Result<(ScriptMode, Vec<(f32, String)>), String> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap_or("").split_whitespace().collect();
    let mode = match head[..] {
        ["headless", steps, ref rest @ ..] => ScriptMode::Headless {
            steps: steps.parse().map_err(|_| format!("headless {steps}: not a step count"))?,
            press: rest.iter().find_map(|w| w.strip_prefix("press=")).unwrap_or("").split(',').map(String::from).collect(),
        },
        ["window"] => ScriptMode::Window,
        _ => return Err("the first line is `headless <steps> [press=a,b]` or `window`".into()),
    };
    let commands = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (t, c) = l.split_once(' ').ok_or_else(|| format!("{l}: want `<t> <command>`"))?;
            Ok((t.parse().map_err(|_| format!("{t}: not a time"))?, c.trim().to_string()))
        })
        .collect::<Result<_, String>>()?;
    Ok((mode, commands))
}

/// Runs a module-side request now; `None` when its answer comes later (a scenario's, when it ends).
fn handle(main: &mut Main, r: &Request) -> Option<Result<String, String>> {
    let d = match data::get() {
        Some(d) => d,
        None => return Some(Err(format!("NM's data isn't loaded: {}", data::how_to()))),
    };
    let bench = match &mut main.bench {
        Some(b) => b,
        None => match Bench::new(&d) {
            Ok(b) => main.bench.insert(b),
            Err(e) => return Some(Err(e)),
        },
    };
    match r.kind.as_str() {
        "nm" => Some(bench.send(r.text.trim())),
        "bench" => Some(match r.text.trim() {
            "reset" => Bench::new(&d).map(|b| {
                main.bench = Some(b);
                "the bench agent is new: Homer standing at the room's start, held".to_string()
            }),
            "stop" => {
                main.bench = None;
                Ok("the bench agent is gone".into())
            }
            "cancel" => Ok(if bench.cancel() { "the scenario is cancelled" } else { "no scenario runs" }.into()),
            t => Err(format!("bench {t}: reset | stop | cancel")),
        }),
        "run" => {
            if main.run.is_some() {
                return Some(Err("a scenario runs already (bench cancel)".into()));
            }
            match parse_run(&r.text).and_then(|(mode, lines)| bench.run(&d, mode, lines)) {
                Ok(()) => {
                    main.run = Some((r.conn, r.id));
                    None
                }
                Err(e) => Some(Err(e)),
            }
        }
        k => Some(Err(format!("{k}: not the module's"))),
    }
}

/// `Think`: takes the queued requests, steps the bench, publishes telemetry. The console commands and Lua among the
/// requests come back to be run by Lua.
///
/// # Safety
/// `l` is a live Lua state, called from its own thread.
unsafe fn think(api: &Api, l: *mut LuaState, now: f64, tick: f64, dt: f32, gravity: f32) -> Result<c_int, String> {
    let Some((shared, _)) = running() else {
        unsafe { (api.createtable)(l, 0, 0) };
        return Ok(1);
    };
    let requests: Vec<Request> = lock(&shared.requests).drain(..).collect();
    let mut main = lock(&MAIN);
    let mut for_lua = vec![];
    for r in requests {
        if r.kind == "exec" || r.kind == "lua" {
            for_lua.push(r);
        } else if let Some(answer) = handle(&mut main, &r) {
            shared.reply(r.conn, r.id, answer);
        }
    }
    let mut fields: Vec<(&'static str, String)> = vec![
        ("t", format!("{now}")),
        ("tick", format!("{tick}")),
        ("frame_dt", json::num(dt)),
        ("hover", step::count().to_string()),
        // ADR 0007: compensation comes with FOLLOW (M4); the gravities it works from are here already.
        ("gravity", format!("{{\"nm\":{},\"source\":{},\"compensation\":null}}", json::num(9.8), json::num(gravity))),
        // The FOLLOW agents on ragdolls (M4) go here.
        ("agents", "[]".into()),
    ];
    let d = data::get();
    let main = &mut *main;
    match (&mut main.bench, d) {
        (Some(b), Some(d)) => {
            let finished = b.think(&d, dt);
            match finished {
                Ok(Some(f)) => {
                    if let Some((conn, id)) = main.run.take() {
                        let note = if f.unsupported.is_empty() {
                            String::new()
                        } else {
                            format!("# dms: not run on the bench: {}\n", f.unsupported.join("; "))
                        };
                        shared.reply(conn, id, Ok(format!("{note}{}", f.rec)));
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    if let Some((conn, id)) = main.run.take() {
                        shared.reply(conn, id, Err(format!("the bench failed: {e}")));
                    }
                    main.bench = None;
                }
            }
            fields.push(("bench", "true".into()));
            if let Some(b) = &main.bench {
                b.telemetry(&mut fields)?;
            }
        }
        _ => fields.push(("bench", "false".into())),
    }
    publish(&shared, fields);
    unsafe {
        (api.createtable)(l, for_lua.len() as c_int, 0);
        for (i, r) in for_lua.iter().enumerate() {
            (api.createtable)(l, 0, 4);
            api.set_number(l, c"conn", r.conn as f64);
            api.set_number(l, c"id", r.id as f64);
            api.set_str(l, c"kind", &r.kind);
            api.set_str(l, c"text", &r.text);
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

/// Keeps the frame's telemetry as the snapshot and sends it to every watcher.
fn publish(shared: &Shared, fields: Vec<(&'static str, String)>) {
    let watchers: Vec<(u64, Option<Vec<String>>)> = lock(&shared.watchers).iter().map(|(c, f)| (*c, f.clone())).collect();
    for (conn, only) in watchers {
        let line = json::object(&fields, only.as_deref());
        shared.send(conn, frame(0, "tel", &line));
    }
    *lock(&shared.snapshot) = fields;
}

pub const NATIVES: &[(&CStr, lua::CFunction)] = &[
    (c"DevStart", dev_start),
    (c"DevStop", dev_stop),
    (c"DevPort", dev_port),
    (c"DevThink", dev_think),
    (c"DevReply", dev_reply),
];

native!(dev_start => dev_start_impl);
/// `DMS.DevStart(port, token)`: the channel on 127.0.0.1:`port` (restarted if it runs).
unsafe fn dev_start_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let port = unsafe { api.check_number(l, 1)? };
    let token = unsafe { api.check_string(l, 2)? };
    if !(1.0..=65535.0).contains(&port) {
        return Err("bad argument #1 (a port, 1 to 65535)".into());
    }
    start(port as u16, &token)?;
    Ok(0)
}

native!(dev_stop => dev_stop_impl);
/// `DMS.DevStop()`.
unsafe fn dev_stop_impl(_api: &Api, _l: *mut LuaState) -> Result<c_int, String> {
    stop();
    Ok(0)
}

native!(dev_port => dev_port_impl);
/// `DMS.DevPort()`: the port the channel listens on, nil when it's off.
unsafe fn dev_port_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    match running() {
        Some((_, port)) => unsafe { (api.pushnumber)(l, f64::from(port)) },
        None => unsafe { (api.pushnil)(l) },
    }
    Ok(1)
}

native!(dev_think => dev_think_impl);
/// `DMS.DevThink(curTime, tickCount, frameTime, gravity)`: the channel's work for this frame (gravity: Source's, m/s^2,
/// up positive). The console commands and Lua that came in, `{conn, id, kind, text}` each, for Lua to run and answer
/// with `DMS.DevReply`.
unsafe fn dev_think_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let now = unsafe { api.check_number(l, 1)? };
    let tick = unsafe { api.check_number(l, 2)? };
    let dt = unsafe { api.check_number(l, 3)? } as f32;
    let gravity = unsafe { api.check_number(l, 4)? } as f32;
    unsafe { think(api, l, now, tick, dt.clamp(0.0, 1.0), gravity) }
}

native!(dev_reply => dev_reply_impl);
/// `DMS.DevReply(conn, id, ok, text)`: the answer to a request `DevThink` handed out.
unsafe fn dev_reply_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let conn = unsafe { api.check_number(l, 1)? } as u64;
    let id = unsafe { api.check_number(l, 2)? } as u64;
    let ok = unsafe { (api.toboolean)(l, 3) } != 0;
    let text = unsafe { api.check_string(l, 4)? };
    if let Some((shared, _)) = running() {
        shared.reply(conn, id, if ok { Ok(text) } else { Err(text) });
    }
    Ok(0)
}
