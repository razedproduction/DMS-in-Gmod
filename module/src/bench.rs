//! The bench agent (ROADMAP M2.5): Homer's agent in LEAD on the Playroom's floor, stepped in real time from
//! `Think`, so `dms-ctl` can drive it and run the Playroom's scenarios (`golden/scenarios/`) on the server, recorded
//! in the Playroom's `--rec` format. In LEAD NM moves its own body: no game physics is involved. The FOLLOW agent
//! on the server's ragdoll comes with M4 and takes its place.
//!
//! The Playroom's window (`step`, `scripted`, `tick`): commands fire at the first frame whose elapsed time reaches
//! theirs, before that frame's steps; the steps run on an accumulator (1/60 s, at most 0.25 s a frame), not before
//! the first command (`held`). Its headless run presses `balance` (and `--press`) at step 0 and shoots at step 60.

use std::collections::VecDeque;
use std::fmt::Write;

use crate::data::{self, Data};
use crate::nm::{self, Mode};

/// The Playroom's camera, from which `!poke` pushes (`Sim::eye`), and its poke (N s).
const EYE: [f32; 3] = [0.0, -4.0, 1.5];
const POKE: f32 = 8.0;
/// The Playroom's headless shot (`headless`).
const HEADLESS_SHOT_STEP: u64 = 60;
/// The Playroom's `get-up` button cycles these (`GETUP_MODES`), from the first.
const GETUP_MODES: [&str; 4] = ["animation", "muscles", "off", "learned"];
const REC_HEADER: &str = "# game: frame t pelvis xyz |v| touches | ours: t com xyz |v|   (# lines: the log)";

/// How a scenario runs (its `#!` line).
pub enum ScriptMode {
    /// `#! headless <steps> [press=a,b]`.
    Headless { steps: u64, press: Vec<String> },
    /// `#! window`: the commands at their times.
    Window,
}

/// A scenario running on the bench.
struct Script {
    mode: ScriptMode,
    lines: VecDeque<(f32, String)>,
    /// Time since the scenario started (the Playroom's `time.elapsed_secs()`).
    elapsed: f32,
    rec: String,
    unsupported: Vec<String>,
    done: bool,
}

pub struct Bench {
    world: nm::World,
    agent: nm::Agent,
    head: usize,
    /// NM steps of this agent (the Playroom's `steps`).
    steps: u64,
    acc: f32,
    held: bool,
    /// The Playroom's get-up mode (only logged).
    getup: usize,
    script: Option<Script>,
}

/// A scenario's outcome: its `.rec`, and the commands the bench couldn't run.
pub struct Finished {
    pub rec: String,
    pub unsupported: Vec<String>,
}

fn v3(x: [f32; 3]) -> String {
    format!("{:.3},{:.3},{:.3}", x[0], x[1], x[2])
}

fn new_agent(d: &Data, z: f32) -> Result<nm::Agent, String> {
    let mut a = nm::Agent::new(&d.build, [0.0, 0.0, z], 0.0, nm::PLAYROOM_SEED)?;
    a.set_mode(Mode::Lead)?;
    Ok(a)
}

impl Bench {
    /// Homer standing at the room's start, held until the first command, as in the Playroom.
    pub fn new(d: &Data) -> Result<Bench, String> {
        let head = (0..nm::PARTS).find(|&i| nm::part_name(i).is_ok_and(|n| n == "Head")).ok_or("no part Head")?;
        Ok(Bench { world: data::floor_world(d)?, agent: new_agent(d, 0.0)?, head, steps: 0, acc: 0.0, held: true, getup: 0, script: None })
    }

    /// The Playroom's `say`: a `#` line of the record, at the current step.
    fn say(&mut self, s: &str) {
        if let Some(r) = self.script.as_mut() {
            let _ = writeln!(r.rec, "# {:.2} {s}", self.steps as f32 / 60.0);
        }
    }

    fn unsupported(&mut self, what: &str) {
        self.say(&format!("dms  unsupported on the bench: {what}"));
        if let Some(r) = self.script.as_mut() {
            r.unsupported.push(what.to_string());
        }
    }

    /// A message line to the agent (the Playroom's `send`): `<message> [param=value ...]`.
    pub fn send(&mut self, line: &str) -> Result<String, String> {
        self.held = false;
        let sent = nm::parse_message(line).and_then(|(name, params)| {
            let params: Vec<(&str, nm::Value)> = params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            self.agent.send(&self.world, &name, &params)
        });
        let short: String = line.chars().take(110).collect();
        match sent {
            Ok(unused) => {
                let kept = if unused > 0 { format!("  ({unused} kept, not used by the port)") } else { String::new() };
                self.say(&format!("ours  MSG {short}{kept}"));
                Ok(format!("sent: {line}{kept}"))
            }
            Err(e) => {
                self.say(&format!("ours  {e}"));
                Err(e)
            }
        }
    }

    /// The Playroom's `button`, for what the bench can do: behaviours that are message lines only, `fall`,
    /// `get-up` (no get-up animation here: always off), `RESET pose`.
    fn button(&mut self, d: &Data, b: &str) {
        let lines: &[&str] = match b {
            // The Playroom's get-up modes cycle; DMS has no get-up at all, the mode is only logged as there.
            "get-up" => {
                self.getup = (self.getup + 1) % GETUP_MODES.len();
                return self.say(&format!("ours  get-up: {}", GETUP_MODES[self.getup]));
            }
            "fall" => {
                if self.steps > 0 {
                    return self.unsupported("fall after steps (only from the build's pose)");
                }
                // The Playroom's `fall`: a new agent 3 m up (smartFall only with its additional reactions, off).
                match new_agent(d, 3.0) {
                    Ok(a) => (self.agent, self.held) = (a, false),
                    Err(e) => return self.say(&format!("ours  fall: {e}")),
                }
                return self.say("ours  fall: from 3 m");
            }
            "RESET pose" => {
                match new_agent(d, 0.0) {
                    Ok(a) => (self.agent, self.held) = (a, true),
                    Err(e) => return self.say(&format!("ours  reset: {e}")),
                }
                return self.say("room  RESET pose");
            }
            _ => match crate::follow::button_lines(b) {
                Some(l) => l,
                None => return self.unsupported(&format!("!button {b}")),
            },
        };
        self.say(&format!("room  {b}"));
        self.held = false;
        for l in lines {
            let _ = self.send(l);
        }
    }

    /// `!poke <part>`: `applyImpulse` of 8 N s into the part, away from the Playroom's camera. Its camera moves in the
    /// window; the bench takes where it starts, so the direction can differ from the Playroom's record.
    fn poke(&mut self, part: &str) {
        let k = part.parse().unwrap_or(9);
        let Some(p) = self.agent.part_transforms().ok().and_then(|t| t.get(k).map(|f| f.0)) else {
            return self.unsupported(&format!("!poke {part}"));
        };
        let d = [p[0] - EYE[0], p[1] - EYE[1], p[2] - EYE[2]];
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let imp = d.map(|x| x / n * POKE);
        let name = nm::part_name(k).unwrap_or_else(|_| "?".into());
        self.say(&format!("room  poke {name} ({POKE:.1} N s)"));
        let _ = self.send(&format!("applyImpulse start=true partIndex={k} hitPoint={} impulse={}", v3(p), v3(imp)));
    }

    /// One command of a scenario (the Playroom's `scripted`).
    fn command(&mut self, d: &Data, line: &str) {
        let mut w = line.splitn(2, ' ');
        match (w.next().unwrap_or(""), w.next()) {
            ("!button", Some(b)) => self.button(d, b),
            ("!poke", Some(k)) => self.poke(k),
            ("!quit", _) => {
                if let Some(r) = self.script.as_mut() {
                    r.done = true;
                }
            }
            (c, _) if c.starts_with('!') => self.unsupported(line),
            _ => {
                let _ = self.send(line);
            }
        }
    }

    /// Starts a scenario on a new agent: `lines` are its `<t> <command>`s in the order the Playroom runs them.
    pub fn run(&mut self, d: &Data, mode: ScriptMode, lines: Vec<(f32, String)>) -> Result<(), String> {
        let mut lines = lines;
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        (self.agent, self.steps, self.acc, self.held, self.getup) = (new_agent(d, 0.0)?, 0, 0.0, true, 0);
        let headless = match &mode {
            ScriptMode::Headless { press, .. } => Some(press.clone()),
            ScriptMode::Window => None,
        };
        self.script = Some(Script {
            mode,
            lines: lines.into(),
            elapsed: 0.0,
            rec: format!("{REC_HEADER}\n"),
            unsupported: vec![],
            done: false,
        });
        if let Some(press) = headless {
            self.button(d, "balance");
            for b in press.iter().filter(|b| !b.is_empty()) {
                self.button(d, b);
            }
        }
        Ok(())
    }

    /// Drops the running scenario, if any.
    pub fn cancel(&mut self) -> bool {
        self.script.take().is_some()
    }

    /// One server frame of `dt` seconds: the commands due, then the NM steps. A scenario that ended comes back.
    pub fn think(&mut self, d: &Data, dt: f32) -> Result<Option<Finished>, String> {
        if let Some(r) = self.script.as_mut().filter(|r| matches!(r.mode, ScriptMode::Window)) {
            r.elapsed += dt;
            let now = r.elapsed;
            while let Some((_, line)) = self.script.as_mut().filter(|r| !r.done).and_then(|r| {
                r.lines.front().is_some_and(|l| l.0 <= now).then(|| r.lines.pop_front()).flatten()
            }) {
                self.command(d, &line);
            }
        }
        self.acc = (self.acc + dt).min(0.25);
        if self.held {
            self.acc = 0.0;
        }
        // A headless run's length; a window's `!quit` ends it before this frame's steps (the Playroom records none
        // after it).
        let limit = match self.script.as_ref().map(|r| &r.mode) {
            Some(ScriptMode::Headless { steps, .. }) => Some(*steps),
            _ => None,
        };
        let quit = self.script.as_ref().is_some_and(|r| r.done);
        while !quit && self.acc >= nm::DT && limit.is_none_or(|n| self.steps < n) {
            self.acc -= nm::DT;
            if self.steps == HEADLESS_SHOT_STEP && limit.is_some_and(|n| n > HEADLESS_SHOT_STEP) {
                self.unsupported("the headless shot at step 60 (the Playroom's ShotTask, M6)");
            }
            self.tick()?;
        }
        let done = self.script.as_ref().is_some_and(|r| r.done) || limit.is_some_and(|n| self.steps >= n);
        if !done {
            return Ok(None);
        }
        let r = self.script.take().expect("a done script");
        Ok(Some(Finished { rec: r.rec, unsupported: r.unsupported }))
    }

    /// One NM step, and its record line (the Playroom's `tick`).
    fn tick(&mut self) -> Result<(), String> {
        self.agent.step(&self.world, nm::DT)?;
        self.steps += 1;
        if self.script.is_none() {
            return Ok(());
        }
        let line = self.rec_line()?;
        let feedback = self.agent.feedback()?;
        let r = self.script.as_mut().expect("a script");
        r.rec.push_str(&line);
        r.rec.push('\n');
        for (name, slot) in feedback {
            self.say(&format!("ours  FB {name} slot {slot}"));
        }
        Ok(())
    }

    /// `- - - - - - - | t com |v| | p<k> x y z n<contacts> g<gap> [points] | ...` for the two lowest parts. The SDK
    /// gives a part's contact flag (`nm_agent_get_part_contact`), not the Playroom's held contacts (`held_world`):
    /// `n` is that flag, the gap stays 9.000 and there are no points (in FOLLOW the touches come from Box3D, M4).
    fn rec_line(&self) -> Result<String, String> {
        let (com, v) = self.agent.com()?;
        let speed = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let mut low: Vec<(usize, [f32; 3])> = self.agent.part_transforms()?.into_iter().map(|f| f.0).enumerate().collect();
        low.sort_by(|a, b| a.1[2].total_cmp(&b.1[2]));
        let feet = low
            .iter()
            .take(2)
            .map(|&(k, p)| Ok(format!("p{k} {:.3} {:.3} {:.3} n{} g{:.3} []", p[0], p[1], p[2], self.agent.part_contact(k)?, 9.0)))
            .collect::<Result<Vec<_>, String>>()?;
        Ok(format!(
            "- - - - - - - | {:.3} {:.3} {:.3} {:.3} {:.3} | {}",
            self.steps as f32 / 60.0,
            com[0],
            com[1],
            com[2],
            speed,
            feet.join(" | ")
        ))
    }

    /// The agent's state as telemetry fields (JSON values by name).
    pub fn telemetry(&self, out: &mut Vec<(&'static str, String)>) -> Result<(), String> {
        let frames = self.agent.part_transforms()?;
        let (com, comv) = self.agent.com()?;
        let contacts = (0..frames.len()).map(|k| self.agent.part_contact(k)).collect::<Result<Vec<_>, _>>()?;
        let drives = self.agent.part_drives()?;
        let feedback = self.agent.feedback()?;
        out.push(("step", self.steps.to_string()));
        out.push(("nm_t", json::num(self.steps as f32 / 60.0)));
        out.push(("mode", json::string("lead")));
        out.push(("held", self.held.to_string()));
        out.push(("balancing", self.agent.is_balancing()?.to_string()));
        out.push(("fallen", self.agent.has_fallen()?.to_string()));
        out.push(("pelvis_z", json::num(frames.first().map_or(f32::NAN, |f| f.0[2]))));
        out.push(("head_z", json::num(frames.get(self.head).map_or(f32::NAN, |f| f.0[2]))));
        out.push(("com", json::array(com.iter().map(|&x| json::num(x)))));
        out.push(("com_v", json::array(comv.iter().map(|&x| json::num(x)))));
        out.push(("parts", json::array(frames.iter().map(|(p, r)| json::array(p.iter().chain(r).map(|&x| json::num(x)))))));
        out.push(("contacts", json::array(contacts.iter().map(u8::to_string))));
        out.push(("drives", json::array(drives.iter().map(|(f, t)| json::array(f.iter().chain(t).map(|&x| json::num(x)))))));
        out.push(("feedback", json::array(feedback.iter().map(|(n, s)| json::array([json::string(n), s.to_string()])))));
        if let Some(r) = &self.script {
            let left = r.lines.len();
            out.push(("script", format!("{{\"elapsed\":{},\"commands_left\":{left}}}", json::num(r.elapsed))));
        }
        Ok(())
    }
}

/// The bit of JSON telemetry needs.
pub mod json {
    /// A number; NaN and the infinities as null.
    pub fn num(x: f32) -> String {
        if x.is_finite() { format!("{x}") } else { "null".into() }
    }

    pub fn string(s: &str) -> String {
        let mut o = String::with_capacity(s.len() + 2);
        o.push('"');
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\r' => o.push_str("\\r"),
                '\t' => o.push_str("\\t"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }

    pub fn array(items: impl IntoIterator<Item = String>) -> String {
        format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
    }

    /// `{"name":value,...}` of the fields, only the named ones if `only` is given.
    pub fn object(fields: &[(&str, String)], only: Option<&[String]>) -> String {
        let pick = fields.iter().filter(|(k, _)| only.is_none_or(|o| o.iter().any(|n| n == k)));
        format!("{{{}}}", pick.map(|(k, v)| format!("{}:{v}", string(k))).collect::<Vec<_>>().join(","))
    }
}
