//! dms-ctl: the bench server from the command line (ROADMAP M2.5, docs/bench.md). Every command ends by itself.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use dms_ctl::{Bench, Channel, Scenario, parse_duration, rcon::Rcon};

const USAGE: &str = "\
usage: dms-ctl <command>   (the bench: scripts\\server.ps1 start; addresses from .bench/server.txt)
  status                          what answers: RCON, the module's channel
  exec <console command...>       over RCON, with its output; --module: in Think via the module, no output
  lua <code...> | lua -f <file>   Lua in Think: what it prints and returns (RCON lua_run if the module is down);
                                  PowerShell 5.1 drops \" in arguments: use 'single' quotes in Lua, or -f
  nm <message> [param=value ...]  an NM message to the bench agent (Homer in LEAD on the Playroom's floor)
  bench reset|stop|cancel         a new bench agent | none | stop the running scenario
  snapshot [--fields a,b]         the last telemetry (JSON)
  watch --for <time> [--fields a,b] [--every N]
                                  telemetry lines (JSON) for <time> (2s, 500ms, 1m), every N-th
  run <scenario.txt> [--out <file.rec>] [--ref <playroom.rec>] [--lead]
                                  a golden/scenarios/ scenario on Homer in FOLLOW (--lead: the LEAD bench
                                  agent), in real time -> its .rec (default golden/out/source/<name>.rec);
                                  window commands at the steps of the Playroom's record (default
                                  golden/out/<name>.rec)";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = dispatch(args) {
        eprintln!("dms-ctl: {e}");
        std::process::exit(1);
    }
}

/// Takes `--flag value` out of `args`.
fn take(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    let v = args.get(i + 1).cloned();
    args.drain(i..(i + 2).min(args.len()));
    v
}

fn take_switch(args: &mut Vec<String>, flag: &str) -> bool {
    let at = args.iter().position(|a| a == flag);
    at.map(|i| args.remove(i)).is_some()
}

fn dispatch(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err(USAGE.into());
    }
    let cmd = args.remove(0);
    let bench = Bench::load()?;
    match cmd.as_str() {
        "status" => status(&bench),
        "exec" => {
            let module = take_switch(&mut args, "--module");
            let line = need(&args, "exec <console command>")?;
            if module {
                return say(Channel::bench(&bench)?.request("exec", &line, Duration::from_secs(10))?);
            }
            match Rcon::connect(bench.rcon_port, &bench.rcon_password).and_then(|mut r| r.exec(&line)) {
                Ok(out) => {
                    print!("{out}");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("dms-ctl: {e}; through the module instead (no output)");
                    say(Channel::bench(&bench)?.request("exec", &line, Duration::from_secs(10))?)
                }
            }
        }
        "lua" => {
            let code = match take(&mut args, "-f") {
                Some(file) => std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?,
                None => need(&args, "lua <code> | lua -f <file.lua>")?,
            };
            match Channel::bench(&bench) {
                Ok(mut c) => say(c.request("lua", &code, Duration::from_secs(30))?),
                Err(e) => {
                    eprintln!("dms-ctl: {e}; RCON lua_run instead");
                    let mut r = Rcon::connect(bench.rcon_port, &bench.rcon_password)?;
                    print!("{}", r.exec(&format!("lua_run {code}"))?);
                    Ok(())
                }
            }
        }
        "nm" => say(Channel::bench(&bench)?.request("nm", &need(&args, "nm <message> [param=value ...]")?, Duration::from_secs(10))?),
        "bench" => say(Channel::bench(&bench)?.request("bench", &need(&args, "bench reset|stop|cancel")?, Duration::from_secs(10))?),
        "snapshot" => {
            let fields = take(&mut args, "--fields").unwrap_or_default();
            say(Channel::bench(&bench)?.request("snapshot", &fields, Duration::from_secs(5))?)
        }
        "watch" => {
            let time = take(&mut args, "--for").ok_or("watch --for <time>: it has to end by itself")?;
            let duration = parse_duration(&time)?;
            let fields = take(&mut args, "--fields").unwrap_or_default();
            let every: usize = take(&mut args, "--every").map_or(Ok(1), |n| n.parse().map_err(|_| "--every <N>"))?.max(1);
            let mut c = Channel::bench(&bench)?;
            let (mut i, out) = (0usize, std::io::stdout());
            let mut out = out.lock();
            let n = c.watch(&fields, duration, |line| {
                i += 1;
                (i - 1) % every != 0 || writeln!(out, "{line}").is_ok()
            })?;
            if n == 0 {
                return Err(format!("no telemetry in {time} (is the server's Think running? sv_hibernate_think 1)"));
            }
            Ok(())
        }
        "run" => {
            let out = take(&mut args, "--out").map(PathBuf::from);
            let lead = take_switch(&mut args, "--lead");
            let reference = take(&mut args, "--ref").map(PathBuf::from);
            let path = PathBuf::from(need(&args, "run <scenario.txt>")?);
            let s = Scenario::read(&path)?;
            let out = out.unwrap_or_else(|| dms_ctl::repo().join("golden").join("out").join("source").join(format!("{}.rec", s.name)));
            // The Playroom's record of it, for the steps its commands ran at.
            let reference = reference.unwrap_or_else(|| dms_ctl::repo().join("golden").join("out").join(format!("{}.rec", s.name)));
            let theirs = std::fs::read_to_string(&reference).ok().map(|t| dms_ctl::rec::parse(&t)).filter(|r| !r.steps.is_empty());
            eprintln!("dms-ctl: {} ({}{}): about {:.0} s in real time{}", s.name, s.mode, if lead { ", LEAD" } else { ", Homer in FOLLOW" },
                s.seconds(), if theirs.is_some() { "" } else { "; no Playroom record: commands at their times" });
            let start = Instant::now();
            let rec = dms_ctl::run(&mut Channel::bench(&bench)?, &s, theirs.as_ref(), lead)?;
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            std::fs::write(&out, &rec).map_err(|e| format!("{}: {e}", out.display()))?;
            let r = dms_ctl::rec::parse(&rec);
            println!(
                "{}: {} steps to t = {:.3} s in {:.1} s -> {}",
                s.name,
                r.steps.len(),
                r.steps.last().map_or(0.0, |x| x.t),
                start.elapsed().as_secs_f32(),
                out.display()
            );
            for l in rec.lines().filter(|l| l.starts_with("# dms:")) {
                println!("{l}");
            }
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// The rest of the arguments as one line.
fn need(args: &[String], what: &str) -> Result<String, String> {
    if args.is_empty() { Err(format!("usage: dms-ctl {what}")) } else { Ok(args.join(" ")) }
}

fn say(text: String) -> Result<(), String> {
    println!("{text}");
    Ok(())
}

fn status(b: &Bench) -> Result<(), String> {
    println!("bench server: {} (pid {}), log {}", b.dir, b.pid, b.log);
    match Rcon::connect(b.rcon_port, &b.rcon_password) {
        Ok(_) => println!("RCON 127.0.0.1:{}: ok", b.rcon_port),
        Err(e) => println!("RCON 127.0.0.1:{}: {e}", b.rcon_port),
    }
    match Channel::bench(b).and_then(|mut c| c.request("ping", "", Duration::from_secs(5))) {
        Ok(_) => println!("module channel 127.0.0.1:{}: ok", b.dev_port),
        Err(e) => println!("module channel 127.0.0.1:{}: {e}", b.dev_port),
    }
    Ok(())
}
