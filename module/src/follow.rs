//! Homer's FOLLOW agent (ROADMAP M4): NM's brain on the ragdoll's 21 Box3D bodies, stepped from the step callback
//! (`step.rs`).
//!
//! It runs on the crate `delirium` as the Playroom does (docs/adr/0016): the agent, its messages, the outbox its
//! behaviours write and the room's reactions the scenarios meet are the Playroom's code (`delirium-room`: `Sim::tick`,
//! `send_to`, `extras_frame`, `in_air`, `fall`, `poke`); only the body is Box3D's. Nothing here calls Lua or the
//! game (docs/adr/0003): only `IPhysicsNM`.
//!
//! Each NM step (1/60 s, on an accumulator over Source's steps, docs/adr/0008):
//!   1. every part's state from Box3D into NM's doll (`GetBodyState`, `set_body`), NM's own sleep kept unless Box3D
//!      moved the body (in the Playroom nothing but the doll's own physics wakes it);
//!   2. what the Playroom's `tick` does before its step: the scenario's commands of this step, the last step's
//!      outbox (`extras_frame`), `in_air` every other step;
//!   3. `Agent::step_follow`; feedback; the drives (`doll.out`, gravity left out) and armature (`doll.armature`).
//! Every Source step then gets the latest drives, the impulses of `applyImpulse` and gravity's compensation
//! (docs/adr/0007) through `ApplyBodyDrive`.

use std::collections::VecDeque;
use std::fmt::Write;
use std::sync::Arc;

use delirium::collision::Collision;
use delirium::msg::{self, Kind, Value};
use delirium::nm::Agent;
use delirium::prelude::*;
use delirium::ragdoll::{GRAVITY, Ragdoll};

use crate::data::Data;
use crate::ffi::vphysics_nm::{IPhysicsConstraint, IPhysicsObject, Nm};

pub const PARTS: usize = 21;
/// NM's own step.
pub const DT: f64 = 1.0 / 60.0;
/// The Playroom's agents' seed (`Agent::new(.., 0.3)`).
const SEED: f32 = 0.3;
const HEAD: usize = 12;
/// NM steps one Source step may run at most (a hitch, `host_timescale`): the rest of the time is dropped, as the
/// Playroom's window drops what's past its 4 steps a frame.
const MAX_STEPS: u32 = 4;
/// The Playroom's camera, which `!poke` pushes away from (`Sim::eye` as the window starts), and its poke (N s).
const EYE: Vec3 = Vec3::new(0.0, -4.0, 1.5);
const POKE: f32 = 8.0;
/// The Playroom's headless shot (`headless`).
const HEADLESS_SHOT_STEP: u64 = 60;
/// The Playroom's `get-up` button cycles these (`GETUP_MODES`), from the first.
const GETUP_MODES: [&str; 4] = ["animation", "muscles", "off", "learned"];
pub const REC_HEADER: &str = "# game: frame t pelvis xyz |v| touches | ours: t com xyz |v|   (# lines: the log)";
/// Past these a part has blown up (m/s, rad/s): NM's physics caps a part at 80 m/s (`ragdoll::MAX_SPEED`), and no
/// motion of a body turns a part at 300 rad/s.
const MAX_SPEED: f32 = 80.0;
const MAX_SPIN: f32 = 300.0;
/// How long `hybrid` takes to turn the compensation on or off (s, docs/adr/0007).
const BLEND: f32 = 0.1;

/// How gravity's difference is made up (docs/adr/0007, `dms_gravity_mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GravityMode {
    /// Always: Homer lives in NM's 9.8 m/s^2.
    Nm,
    /// While a part touches the world; off in free flight, so he falls as the game's objects do.
    Hybrid,
    /// None: Source's gravity as it is.
    Off,
}

impl GravityMode {
    pub fn parse(s: &str) -> Option<GravityMode> {
        match s.trim() {
            "nm" => Some(GravityMode::Nm),
            "hybrid" => Some(GravityMode::Hybrid),
            "off" => Some(GravityMode::Off),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            GravityMode::Nm => "nm",
            GravityMode::Hybrid => "hybrid",
            GravityMode::Off => "off",
        }
    }
}

/// Something for Lua (`DMS.FollowEvents`).
pub struct Event {
    pub id: u32,
    pub kind: &'static str,
    pub text: String,
    pub value: f32,
}

/// The Playroom's buttons that only send message lines (`Sim::lines`), the ones the bench presses.
pub fn button_lines(b: &str) -> Option<&'static [&'static str]> {
    Some(match b {
        "balance" => &["bodyBalance start=true"],
        "windmill" => &["bodyBalance start=true", "armsWindmillAdaptive start=true"],
        "pedal" => &["pedalLegs start=true"],
        "writhe" => &["bodyWrithe start=true"],
        "rollup" => &["bodyRollUp start=true"],
        "catchfall" => &["catchFall start=true"],
        "highfall" => &["highFall start=true"],
        "stairs" => &["rollDownStairs start=true"],
        "relax" => &["bodyRelax start=true"],
        "stayup" => &["stayUpright start=true stiffness=3"],
        "foetal" => &["bodyFoetal start=true"],
        "STOP all" => &["stopAllBehaviours"],
        _ => return None,
    })
}

/// The Playroom's `v3`: a vector in a message line.
fn v3(v: Vec3) -> String {
    format!("{:.3},{:.3},{:.3}", v.x, v.y, v.z)
}

/// The Playroom's `parse`: a param's text as a value of its kind.
fn parse_value(kind: Kind, v: &str) -> Option<Value> {
    Some(match kind {
        Kind::Float => Value::Float(v.parse().ok()?),
        Kind::Int => Value::Int(v.parse().ok()?),
        Kind::Bool => Value::Bool(match v {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return None,
        }),
        Kind::Vec3 => {
            let f: Vec<f32> = v.split(',').map(|x| x.parse().ok()).collect::<Option<_>>()?;
            Value::Vec3(Vec3::new(*f.first()?, *f.get(1)?, *f.get(2)?))
        }
        Kind::Str => Value::Str(v.to_string()),
    })
}

/// `applyImpulse`'s part, impulse, hit point and equalising from its params, the message's defaults for the rest
/// (as `msg::send` takes them: an int or a float for the part, a float or an int for the amount).
fn impulse_params(ps: &[(&str, Value)]) -> (usize, Vec3, Option<Vec3>, f32) {
    let get = |n: &str| ps.iter().find(|(p, _)| *p == n).map(|(_, v)| v.clone()).or_else(|| msg::get(&msg::State::default(), "applyImpulse", n));
    let num = |v: Option<Value>| match v {
        Some(Value::Float(x)) => Some(x),
        Some(Value::Int(i)) => Some(i as f32),
        _ => None,
    };
    let vec = |v: Option<Value>| match v {
        Some(Value::Vec3(v)) => Some(v),
        _ => None,
    };
    let part = num(get("partIndex")).unwrap_or(0.0).max(0.0) as usize;
    let hit = ps.iter().any(|(p, _)| *p == "hitPoint").then(|| vec(get("hitPoint"))).flatten();
    (part.min(PARTS - 1), vec(get("impulse")).unwrap_or(Vec3::ZERO), hit, num(get("equalizeAmount")).unwrap_or(0.0))
}

/// A scenario running on this agent (the bench, `dms-ctl run`): its commands by NM step, as the Playroom ran them
/// (golden/README.md, docs/adr/0011), and its record in the Playroom's `--rec` format.
pub struct Script {
    /// `(step, command)`, in order: each runs before that step.
    pub commands: VecDeque<(u64, String)>,
    /// The step it ends after (a window's `!quit`, a headless run's length).
    pub end: u64,
    pub headless: bool,
    /// The spawn was the Playroom's `fall`: Homer 3 m up.
    pub fallen_from: Option<f32>,
    rec: String,
    unsupported: Vec<String>,
    getup: usize,
    pub done: bool,
}

/// A finished scenario: its `.rec`, and the commands the bench couldn't run.
pub struct Finished {
    pub rec: String,
    pub unsupported: Vec<String>,
}

impl Script {
    pub fn new(commands: Vec<(u64, String)>, end: u64, headless: bool, fallen_from: Option<f32>) -> Script {
        Script {
            commands: commands.into(),
            end,
            headless,
            fallen_from,
            rec: format!("{REC_HEADER}\n"),
            unsupported: vec![],
            getup: 0,
            done: false,
        }
    }

    pub fn finish(self) -> Finished {
        Finished { rec: self.rec, unsupported: self.unsupported }
    }
}

pub struct Follower {
    pub id: u32,
    data: Arc<Data>,
    world: Arc<Collision>,
    /// Part `k`'s body, and its joint to its parent (null for the root).
    objects: Vec<*mut IPhysicsObject>,
    joints: Vec<*mut IPhysicsConstraint>,
    /// Where NM's frame sits in the game's (m): NM sees `world - origin`. The bench's is the Playroom room's origin.
    origin: Vec3,
    pub agent: Agent,
    msgs: msg::State,
    /// NM steps run (the Playroom's `steps`).
    pub steps: u64,
    acc: f64,
    /// The last step's drives, held until the next (docs/adr/0008).
    drives: Vec<(Vec3, Vec3)>,
    armature: Vec<f32>,
    /// `applyImpulse`'s impulses for the next Source step: part, point from its centre, impulse.
    kicks: Vec<(usize, Vec3, Vec3)>,
    /// Lines from Lua for the next step.
    queued: Vec<String>,
    /// How much of gravity's compensation is on (0 to 1), and the parts the game doesn't pull (`EnableGravity`).
    pub comp: f32,
    pub gravity_off: u32,
    /// Box3D's bodies were told to sleep with NM's doll.
    pub bodies_asleep: bool,
    was_fallen: bool,
    /// The body's velocity a step ago, which the Playroom's room layer acts on (in_air).
    prev_com_v: Option<Vec3>,
    pub script: Option<Script>,
    /// Physics time since the last NM step (s), the last Source step's length, and how NM's own physics would have
    /// moved the body over that step: each part's velocity and spin before it and after (`doll.motion` once
    /// `step_follow` has run its prediction). The next step's state from Box3D against it is `pred_err`: where
    /// Box3D's physics parts from NM's (ROADMAP M4, docs/adr/0008).
    since: f64,
    pub timestep: f32,
    before: Vec<(Vec3, Vec3)>,
    predicted: Vec<(Vec3, Vec3)>,
    pub pred_err: Vec<(Vec3, Vec3)>,
    /// How far each joint is into NM's limits on Box3D's state, by part: a ball's swing (its ellipse, 1 on the
    /// edge) and twist (of its range), a hinge's angle (of the nearer bound's).
    limit_use: Vec<(f32, f32)>,
}

// SAFETY: the pointers are only used on the game's main thread, where the physics and Lua both run.
unsafe impl Send for Follower {}

impl Follower {
    /// NM's agent on `objects` (NM's part order) as they are now, facing `yaw` (rad) in NM's frame at `origin`.
    ///
    /// # Safety
    /// The objects and joints are live and stay so until the follower is dropped (`step::unfollow`).
    pub unsafe fn new(
        nm: &Nm,
        id: u32,
        data: Arc<Data>,
        world: Arc<Collision>,
        objects: Vec<*mut IPhysicsObject>,
        joints: Vec<*mut IPhysicsConstraint>,
        origin: Vec3,
        yaw: f32,
    ) -> Result<Follower, String> {
        if objects.len() != PARTS || joints.len() != PARTS {
            return Err(format!("{PARTS} parts and joints expected"));
        }
        let frames = objects
            .iter()
            .map(|&o| {
                // SAFETY: a live object (the caller's promise).
                let s = unsafe { nm.body_state(o) }.ok_or("GetBodyState failed")?;
                let (x, q) = (Vec3::from_array(s.position) - origin, Quat::from_array(s.rotation));
                if !(x.is_finite() && q.is_finite()) {
                    return Err("a part's state isn't finite".to_string());
                }
                Ok(Transform::from_translation(x).with_rotation(q.normalize()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        // As the Playroom makes its agents (`Agent::new(build, Ragdoll::new(build, frames, 0), facing, 0.3)`).
        let doll = Ragdoll::new(&data.homer, &frames, Vec3::ZERO);
        let agent = Agent::new(&data.homer, doll, Quat::from_rotation_z(yaw), SEED);
        Ok(Follower {
            id,
            world,
            objects,
            joints,
            origin,
            agent,
            msgs: msg::State::default(),
            steps: 0,
            // The first Source step runs a step: NM drives the body from the start.
            acc: DT,
            drives: vec![(Vec3::ZERO, Vec3::ZERO); PARTS],
            armature: vec![0.0; PARTS],
            kicks: vec![],
            queued: vec![],
            comp: 1.0,
            gravity_off: 0,
            bodies_asleep: false,
            was_fallen: false,
            prev_com_v: None,
            script: None,
            since: 0.0,
            timestep: 0.0,
            before: vec![],
            predicted: vec![],
            pred_err: vec![],
            limit_use: vec![],
            data,
        })
    }

    pub fn objects(&self) -> &[*mut IPhysicsObject] {
        &self.objects
    }

    pub fn origin(&self) -> Vec3 {
        self.origin
    }

    /// A message line for the next step (`<message> [param=value ...]`), checked now.
    pub fn queue(&mut self, line: &str) -> Result<(), String> {
        let name = line.split_whitespace().next().ok_or("an empty message")?;
        if msg::find(name).is_none() {
            return Err(format!("no message {name}"));
        }
        self.queued.push(line.to_string());
        Ok(())
    }

    /// NM's doll is asleep (the Playroom's `mode` 2): no behaviours, no drives.
    pub fn asleep(&self) -> bool {
        self.agent.doll.mode == 2 && self.agent.doll.asleep
    }

    /// One Source step of `timestep` s: the NM steps it's due, then the drives for it.
    ///
    /// # Safety
    /// The objects and joints are live.
    pub unsafe fn tick(&mut self, nm: &Nm, timestep: f32, g_src: Vec3, mode: GravityMode, events: &mut Vec<Event>) -> Result<(), String> {
        self.timestep = timestep;
        self.acc += f64::from(timestep);
        let mut n = 0;
        while self.acc + 1e-9 >= DT {
            self.acc -= DT;
            n += 1;
            if n > MAX_STEPS {
                self.acc = 0.0;
                break;
            }
            unsafe { self.nm_step(nm, events)? };
        }
        unsafe { self.apply(nm, timestep, g_src, mode) };
        self.since += f64::from(timestep);
        Ok(())
    }

    /// One NM step (the Playroom's `tick`, with the body's state from Box3D).
    unsafe fn nm_step(&mut self, nm: &Nm, events: &mut Vec<Event>) -> Result<(), String> {
        let dt = DT as f32;
        let (mode, asleep) = (self.agent.doll.mode, self.agent.doll.asleep);
        let mut moved = (0.0f32, 0.0f32, 0.0f32);
        for (k, &o) in self.objects.iter().enumerate() {
            // SAFETY: a live object (the caller's promise).
            let s = unsafe { nm.body_state(o) }.ok_or("GetBodyState failed")?;
            let (x, q) = (Vec3::from_array(s.position) - self.origin, Quat::from_array(s.rotation));
            let (v, w) = (Vec3::from_array(s.velocity), Vec3::from_array(s.angular_velocity));
            if !(x.is_finite() && q.is_finite() && v.is_finite() && w.is_finite()) || q.length_squared() < 0.5 {
                return Err(format!("part {k}'s state isn't finite"));
            }
            // A blow-up is as bad as a NaN: faster than NM's own physics ever lets a part go.
            if v.length() > MAX_SPEED || w.length() > MAX_SPIN {
                return Err(format!("part {k} blew up ({:.0} m/s, {:.0} rad/s)", v.length(), w.length()));
            }
            if k == 0 {
                (moved.0, moved.1) = (v.length_squared(), w.length_squared());
            }
            moved.2 += (v.length_squared() + w.length_squared()) / PARTS as f32;
            self.agent.doll.set_body(k, x, q, v, w);
        }
        // NM's prediction, scaled to the physics time that went by, against what Box3D did.
        if self.predicted.len() == PARTS && self.since > 0.0 {
            let s = (self.since / DT) as f32;
            self.pred_err = (0..PARTS)
                .map(|k| {
                    let (v, w) = self.agent.doll.motion(k);
                    let ((v0, w0), (v1, w1)) = (self.before[k], self.predicted[k]);
                    (v - (v0 + (v1 - v0) * s), w - (w0 + (w1 - w0) * s))
                })
                .collect();
        }
        self.since = 0.0;
        self.before = (0..PARTS).map(|k| self.agent.doll.motion(k)).collect();
        self.limit_use = self.limit_use();
        // `set_body` wakes the doll; the Playroom's only wakes when it moves (`Ragdoll::sleep`'s tolerances).
        let [v2, w2, mean2] = self.agent.doll.sleep_tol;
        if moved.0 <= v2 && moved.1 <= w2 && moved.2 <= mean2 {
            (self.agent.doll.mode, self.agent.doll.asleep) = (mode, asleep);
        }
        // A finished scenario's agent goes on as any other; its record is complete.
        if self.script.as_ref().is_some_and(|s| !s.done) {
            self.script_step();
        }
        for line in std::mem::take(&mut self.queued) {
            self.send(&line);
        }
        // The Playroom's `tick`: the outbox first, `in_air` on even steps, then the step. Its tick sees the body a
        // physics step before its behaviours do (it steps physics first, its new agents' first step without any), so
        // `in_air` takes the last step's velocity.
        let com_v = self.agent.doll.com().1;
        self.extras_frame();
        if self.steps % 2 == 0 {
            self.in_air(self.prev_com_v.unwrap_or(com_v));
        }
        self.prev_com_v = Some(com_v);
        // The record's line of the step about to run: by the same token, the state the step starts from.
        let line = self.script.as_ref().filter(|s| !s.done).map(|_| self.rec_line(self.steps + 1));
        // NM's balancer steadies the body with impulses (`nmdbm::stabilise` while `bodyBalance` runs: the feet one
        // way, the pelvis the other), which its step hands to its own physics (`doll.impulse`), not to `doll.out`:
        // in FOLLOW they'd be lost. The same function on the same state (the step's, before it runs) gives them;
        // they're linear in its gain squared, which the step itself sets (`Character::stabilise`).
        let unit = self.agent.character().map(|ch| {
            let frames: Vec<Transform> = self.agent.doll.frames().collect();
            let body = delirium::nmdbm::Body { ch, build: &self.data.homer, doll: &self.agent.doll, frames: &frames };
            delirium::nmdbm::stabilise(&body, 1.0, dt)
        });
        self.agent.step_follow(&self.data.homer, &self.world, dt, &[], &[]);
        self.steps += 1;
        if let (Some(unit), Some((gain, _))) = (unit, self.agent.character().and_then(|ch| ch.stabilise)) {
            for (k, j) in unit.into_iter().enumerate().filter(|(_, j)| *j != Vec3::ZERO) {
                self.kicks.push((k, Vec3::ZERO, j * (gain * gain)));
            }
        }
        self.predicted = (0..PARTS).map(|k| self.agent.doll.motion(k)).collect();
        if let (Some(line), Some(s)) = (line, self.script.as_mut()) {
            s.rec.push_str(&line);
            s.rec.push('\n');
        }
        for f in &self.agent.feedback {
            let line = format!("ours  FB {} slot {}", f.name, f.slot);
            if let Some(s) = self.script.as_mut().filter(|s| !s.done) {
                let _ = writeln!(s.rec, "# {:.2} {line}", self.steps as f32 / 60.0);
            }
            events.push(Event { id: self.id, kind: "feedback", text: format!("{} {}", f.name, f.slot), value: f.value });
        }
        if self.agent.fallen != self.was_fallen {
            self.was_fallen = self.agent.fallen;
            events.push(Event { id: self.id, kind: if self.agent.fallen { "fallen" } else { "up" }, text: String::new(), value: 0.0 });
        }
        let doll = &self.agent.doll;
        if doll.out.len() < PARTS || doll.out.iter().any(|(f, t)| !(f.is_finite() && t.is_finite())) {
            return Err("NM's drives aren't finite".into());
        }
        self.drives.copy_from_slice(&doll.out[..PARTS]);
        for k in 1..PARTS {
            let a = doll.armature.get(k).copied().unwrap_or(0.0);
            if !(a.is_finite() && a >= 0.0) {
                return Err(format!("joint {k}'s armature isn't finite"));
            }
            if a != self.armature[k] {
                self.armature[k] = a;
                // SAFETY: a live constraint of the ragdoll.
                unsafe { nm.set_joint_armature(self.joints[k], a) };
            }
        }
        if self.asleep() != self.bodies_asleep {
            self.bodies_asleep = self.asleep();
            events.push(Event { id: self.id, kind: if self.bodies_asleep { "sleep" } else { "wake" }, text: String::new(), value: 0.0 });
        }
        Ok(())
    }

    /// The drives of one Source step: NM's (held), the pending impulses, gravity's compensation. None while NM's
    /// doll sleeps: the bodies sleep with it (Lua puts them to sleep on the `sleep` event), and a drive would wake
    /// them.
    unsafe fn apply(&mut self, nm: &Nm, timestep: f32, g_src: Vec3, mode: GravityMode) {
        let touching = self.agent.doll.touch.iter().any(|&t| t != 0);
        let target = match mode {
            GravityMode::Nm => 1.0,
            GravityMode::Hybrid => f32::from(touching),
            GravityMode::Off => 0.0,
        };
        self.comp = if mode == GravityMode::Hybrid {
            let step = timestep / BLEND;
            (self.comp + (target - self.comp).clamp(-step, step)).clamp(0.0, 1.0)
        } else {
            target
        };
        if self.asleep() && self.kicks.is_empty() {
            return;
        }
        let g_nm = Vec3::NEG_Z * GRAVITY;
        let mut forces: Vec<(Vec3, Vec3)> = if self.asleep() { vec![(Vec3::ZERO, Vec3::ZERO); PARTS] } else { self.drives.clone() };
        for (k, f) in forces.iter_mut().enumerate().filter(|_| !self.asleep()) {
            let g = if self.gravity_off >> k & 1 != 0 { Vec3::ZERO } else { g_src };
            f.0 += (g_nm - g) * (self.agent.doll.part_mass(k) * self.comp);
        }
        for (k, arm, j) in self.kicks.drain(..) {
            let f = j / timestep;
            forces[k].0 += f;
            forces[k].1 += arm.cross(f);
        }
        for (&o, (f, t)) in self.objects.iter().zip(&forces) {
            // SAFETY: a live object; the vectors are finite (checked as they came).
            unsafe { nm.apply_body_drive(o, Some(&f.to_array()), Some(&t.to_array())) };
        }
    }

    /// [`Follower::limit_use`] of the doll's state (NM's limits as `ragdoll.rs`'s `lean_limit` reads them, docs/adr/0014).
    fn limit_use(&self) -> Vec<(f32, f32)> {
        let frames: Vec<Transform> = self.agent.doll.frames().collect();
        let mut out = vec![(0.0, 0.0); PARTS];
        for j in &self.data.body.joints {
            let p = frames[j.parent].rotation * j.frame_parent;
            let c = frames[j.part].rotation * j.frame_child;
            let r = (p.inverse() * c).normalize();
            out[j.part] = if j.hinge {
                let (axis, angle) = r.to_axis_angle();
                let a = if axis.z < 0.0 { -angle } else { angle };
                let a = if a > std::f32::consts::PI { a - std::f32::consts::TAU } else { a };
                let bound = if a >= 0.0 { j.limits[1] } else { j.limits[0] };
                (if bound.abs() > 1e-6 { a / bound } else { 0.0 }, 0.0)
            } else {
                // Swing takes P's Z onto C's Z about an axis in P's XY plane; the twist is what's left.
                let z = r * Vec3::Z;
                let swing = Quat::from_rotation_arc(Vec3::Z, z);
                let twist = swing.inverse() * r;
                let twist = if twist.w < 0.0 { -twist } else { twist };
                let (s, t) = swing.to_axis_angle();
                let e = (t * s.y / j.limits[0]).powi(2) + (t * s.x / j.limits[1]).powi(2);
                let tw = 2.0 * twist.z.atan2(twist.w);
                (e.sqrt(), if j.limits[2] > 1e-6 { tw / j.limits[2] } else { 0.0 })
            };
        }
        out
    }

    /// Takes NM off the ragdoll: no armature (the drives stop with the callback).
    ///
    /// # Safety
    /// The joints are live.
    pub unsafe fn release(&self, nm: &Nm) {
        for &c in self.joints.iter().skip(1) {
            // SAFETY: a live constraint of the ragdoll.
            unsafe { nm.set_joint_armature(c, 0.0) };
        }
    }

    /// The Playroom's `say`: a `#` line of the scenario's record, at the current step.
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

    /// The Playroom's `send_to` (Homer's): a message line, parsed as its console does, through `msg::send`.
    pub fn send(&mut self, line: &str) {
        let mut w = line.split_whitespace();
        let Some(name) = w.next() else { return };
        let Some(m) = msg::find(name) else { return self.say(&format!("no message {name}")) };
        let mut ps: Vec<(&str, Value)> = vec![];
        for kv in w {
            let Some((k, v)) = kv.split_once('=') else { return self.say(&format!("{kv}: want name=value")) };
            let kind = if k == "start" { Some(Kind::Bool) } else { m.params.iter().find(|p| p.name == k).map(|p| p.kind) };
            let Some(kind) = kind else { return self.say(&format!("{name} has no param {k}")) };
            match parse_value(kind, v) {
                Some(v) => ps.push((k, v)),
                None => return self.say(&format!("{k}: not a {kind:?}")),
            }
        }
        let short: String = line.chars().take(110).collect();
        // A one-shot message's state is reset once it's sent: the impulse's params come from the line itself.
        let kick = (name == "applyImpulse").then(|| impulse_params(&ps));
        match msg::send(&mut self.agent, &self.data.homer, &self.world, &mut self.msgs, name, &ps) {
            Ok(unused) => {
                let kept = if unused > 0 { format!("  ({unused} kept, not used by the port)") } else { String::new() };
                self.say(&format!("ours  MSG {short}{kept}"));
            }
            Err(e) => self.say(&format!("ours  {name}: {e:?}")),
        }
        if let Some((k, impulse, hit, equalize)) = kick {
            self.kick(k, impulse, hit, equalize);
        }
    }

    /// `applyImpulse` moves the Playroom's doll (`equalized_impulse`, solved in its physics step); in FOLLOW the
    /// body is Box3D's, so the same impulse goes on it, as a force over the next Source step.
    fn kick(&mut self, k: usize, impulse: Vec3, hit: Option<Vec3>, equalize: f32) {
        let doll = &self.agent.doll;
        let Some(centre) = doll.frames().nth(k).map(|f| f.translation) else { return };
        let mean = doll.mass() / PARTS as f32;
        let s = 1.0 - equalize + equalize * doll.part_mass(k) / mean;
        let (at, j) = (hit.unwrap_or(centre), impulse * s);
        if at.is_finite() && j.is_finite() {
            self.kicks.push((k, at - centre, j));
        }
    }

    /// The Playroom's `extras_frame`: what the behaviours sent each other last step.
    fn extras_frame(&mut self) {
        for l in std::mem::take(&mut self.agent.extras.outbox) {
            match l.strip_prefix("#say ") {
                Some(s) => self.say(s),
                None => self.send(&l),
            }
        }
    }

    /// The Playroom's `in_air` with its defaults (no additional reactions, no tasks, alive): falling faster than
    /// 6 m/s with no extra running sends `stopAllBehaviours` and the game's `NMHighFall` task.
    fn in_air(&mut self, com_v: Vec3) {
        if !self.agent.extras.on.is_empty() || self.agent.high_falling() || com_v.z > -6.0 {
            return;
        }
        self.send("stopAllBehaviours");
        delirium::nm::task_high_fall(&mut self.agent, &self.data.homer, &self.world, &self.data.tp, "*", false);
        self.say("ours  in the air: the game's NMHighFall");
    }

    /// The scenario's part of a step, before it: its end, this step's commands.
    fn script_step(&mut self) {
        let Some(s) = self.script.as_mut() else { return };
        if self.steps >= s.end {
            s.done = true;
            s.commands.clear();
            return;
        }
        if s.headless && self.steps == HEADLESS_SHOT_STEP {
            self.unsupported("the headless shot at step 60 (the Playroom's ShotTask, M6)");
        }
        while let Some(c) = self.script.as_mut().and_then(|s| s.commands.front().is_some_and(|c| c.0 <= self.steps).then(|| s.commands.pop_front()).flatten()) {
            self.command(&c.1);
        }
    }

    /// One command of a scenario (the Playroom's `scripted`).
    fn command(&mut self, line: &str) {
        let mut w = line.splitn(2, ' ');
        match (w.next().unwrap_or(""), w.next()) {
            ("!button", Some(b)) => self.button(b.trim()),
            ("!poke", Some(a)) => self.poke(a),
            ("!quit", _) => {}
            (c, _) if c.starts_with('!') => self.unsupported(line),
            _ => self.send(line),
        }
    }

    /// The Playroom's `button`, for what the bench can do.
    fn button(&mut self, b: &str) {
        match b {
            "get-up" => {
                // DMS has no get-up yet (M6): the mode is only logged, as the Playroom logs it.
                let Some(s) = self.script.as_mut() else { return };
                s.getup = (s.getup + 1) % GETUP_MODES.len();
                let m = GETUP_MODES[s.getup];
                return self.say(&format!("ours  get-up: {m}"));
            }
            "fall" => {
                // The Playroom's `fall`: a new agent 3 m up (smartFall only with its additional reactions). The
                // bench spawned Homer there for it, so only at the start.
                return match self.script.as_ref().and_then(|s| s.fallen_from).filter(|_| self.steps == 0) {
                    Some(h) => self.say(&format!("ours  fall: from {h} m")),
                    None => self.unsupported("fall after the start (Homer is spawned up high for it)"),
                };
            }
            _ => {}
        }
        let Some(lines) = button_lines(b) else { return self.unsupported(&format!("!button {b}")) };
        self.say(&format!("room  {b}"));
        for l in lines {
            self.send(l);
        }
    }

    /// `!poke <part> [impulse]`: the Playroom's `poke`, 8 N s into the part away from its camera. Its camera moves
    /// in the window, so `dms-ctl` hands over the impulse the Playroom's record shows when it has one.
    fn poke(&mut self, a: &str) {
        let mut w = a.split_whitespace();
        let k: usize = w.next().and_then(|k| k.parse().ok()).unwrap_or(9).min(PARTS - 1);
        let Some(at) = self.agent.doll.frames().nth(k).map(|f| f.translation) else { return };
        let given = w.next().and_then(|v| parse_value(Kind::Vec3, v)).and_then(|v| match v {
            Value::Vec3(v) => Some(v),
            _ => None,
        });
        let dir = given.unwrap_or(at - EYE);
        let name = delirium_data::nm_asset::PARTS.get(k).copied().unwrap_or("?");
        self.say(&format!("room  poke {name} ({POKE:.1} N s)"));
        self.send(&format!("applyImpulse start=true partIndex={k} hitPoint={} impulse={}", v3(at), v3(dir.normalize() * POKE)));
    }

    /// `- - - - - - - | t com |v| | p<k> x y z n<held> g<gap> [points] | ...` for the two lowest parts, as the
    /// Playroom's `tick` writes it: the state is Box3D's (just read), the held contacts NM's (`held_world`, from its
    /// last step on that state).
    fn rec_line(&self, step: u64) -> String {
        let d = &self.agent.doll;
        let (com, v) = d.com();
        let mut low: Vec<(usize, Vec3)> = d.frames().map(|f| f.translation).enumerate().collect();
        low.sort_by(|a, b| a.1.z.total_cmp(&b.1.z));
        let feet: Vec<String> = low
            .iter()
            .take(2)
            .map(|&(k, p)| {
                let h = d.held_world(k);
                format!(
                    "p{k} {:.3} {:.3} {:.3} n{} g{:.3} [{}]",
                    p.x,
                    p.y,
                    p.z,
                    h.len(),
                    h.iter().map(|x| x.2).fold(9.0, f32::min),
                    h.iter().map(|x| format!("{:.3},{:.3},{:.3}", x.1.x + 1.5, x.1.y, x.1.z)).collect::<Vec<_>>().join(" ")
                )
            })
            .collect();
        format!(
            "- - - - - - - | {:.3} {:.3} {:.3} {:.3} {:.3} | {}",
            step as f32 / 60.0,
            com.x,
            com.y,
            com.z,
            v.length(),
            feet.join(" | ")
        )
    }

    /// The agent's state as telemetry (a JSON object), in the game's frame (m).
    pub fn telemetry(&self) -> String {
        use crate::bench::json;
        let d = &self.agent.doll;
        let frames: Vec<Transform> = d.frames().collect();
        let (com, v) = d.com();
        let o = self.origin;
        let vec = |p: Vec3| json::array(p.to_array().iter().map(|&x| json::num(x)));
        let fb: Vec<String> = self.agent.feedback.iter().map(|f| json::array([json::string(f.name), f.slot.to_string()])).collect();
        let fields: Vec<(&str, String)> = vec![
            ("id", self.id.to_string()),
            ("step", self.steps.to_string()),
            ("nm_t", json::num(self.steps as f32 / 60.0)),
            ("mode", json::string("follow")),
            ("awake", d.awake().to_string()),
            ("asleep", self.asleep().to_string()),
            ("balancing", self.agent.balancing().to_string()),
            ("fallen", self.agent.fallen.to_string()),
            ("pelvis_z", json::num(frames[0].translation.z + o.z)),
            ("head_z", json::num(frames[HEAD].translation.z + o.z)),
            ("com", vec(com + o)),
            ("com_v", vec(v)),
            ("compensation", json::num(self.comp)),
            ("parts", json::array(frames.iter().map(|f| json::array((f.translation + o).to_array().iter().chain(&f.rotation.to_array()).map(|&x| json::num(x)))))),
            ("contacts", json::array(d.touch.iter().map(u8::to_string))),
            ("drives", json::array(self.drives.iter().map(|(f, t)| json::array(f.to_array().iter().chain(&t.to_array()).map(|&x| json::num(x)))))),
            ("armature", json::array(self.armature.iter().map(|&a| json::num(a)))),
            ("feedback", json::array(fb)),
            ("timestep", json::num(self.timestep)),
            ("state_v", json::array(self.before.iter().map(|(v, w)| json::array(v.to_array().iter().chain(&w.to_array()).map(|&x| json::num(x)))))),
            ("limit_use", json::array(self.limit_use.iter().map(|(a, b)| json::array([json::num(*a), json::num(*b)])))),
            ("pred_err", json::array(self.pred_err.iter().map(|(v, w)| json::array(v.to_array().iter().chain(&w.to_array()).map(|&x| json::num(x)))))),
            ("script", self.script.as_ref().map_or("null".into(), |s| format!("{{\"commands_left\":{},\"end\":{}}}", s.commands.len(), s.end))),
        ];
        json::object(&fields, None)
    }
}
