//! The Playroom room's reaction layer for Homer (ROADMAP M6, docs/adr/0016, 0020): what `delirium-room`'s `Sim` sends
//! its agent on a shot (`fire`, `shot_frame`: ShotTask with the weapon's taskparams set), a blast (`blast_on`,
//! `blast_frame`), fire (`burn`: NM's OnFire), a taser (`zap`), a blow (`land`), a near miss, a car (`car_react`), a
//! fall (`in_air`), a throw (`throw`), its additional reactions (`self_preserve`, `extra_look`, windmill, curl up),
//! health and death (`delirium-data` `ped::damage`) and getting up (`rise`: the game's get-up clips, or the muscles'
//! NMPose). The same calls in the same order; only the inputs are the game's (`React`, from Lua's damage hooks) and
//! the Playroom's own visuals (blood, sounds, rings) are left out.

use std::sync::Mutex;

use delirium::nm::{self, Hit};
use delirium::prelude::*;
use delirium::shottask::{self, ShotTask};
use delirium_data::rng::Rand;
use dms_game::getup::{self, Rise};

use super::{Follower, PARTS, v3};

pub const CHEST: usize = 9;
pub const HEAD: usize = 12;
const THIGH_L: usize = 1;
const THIGH_R: usize = 4;
/// The Playroom's constants (`delirium-room` main.rs).
const WHIZZ: f32 = 1.5;
const BURN: f32 = 6.0;
const BURN_DAMAGE: f32 = 25.0;
const SHOCK: f32 = 2.5;
const INJURY_MAX: f32 = 0.8;
const NEAR_BURST: f32 = 0.5;
const DIVE: (f32, f32) = (3.5, 3.0);
const TUMBLE: f32 = 7.0;
const BLAST_R: f32 = 5.0;
const BLAST_DV: f32 = 15.0;
const BLAST_DAMAGE: f32 = 250.0;
const LEAN_SECS: f32 = 1.5;
const MUSC_STIFF: (f32, f32) = (11.0, 12.0);
/// `MUSC_HELP` (the Playroom's `DELIRIUM_HELP`, unset).
const MUSC_HELP: f32 = 0.5;
pub const START_HEALTH: f32 = 200.0;
/// The Playroom's guns: its name, weaponinfo.xml's, the force without the file.
pub const WEAPONS: [(&str, &str, f32); 13] = [
    ("pistol", "PISTOL", 50.0),
    ("deagle", "DEAGLE", 50.0),
    ("smg", "MP5", 50.0),
    ("ak47", "AK47", 50.0),
    ("sniper", "SNIPERRIFLE", 50.0),
    ("shotgun", "SHOTGUN", 50.0),
    ("rpg", "RLAUNCHER", 50.0),
    ("grenade", "GRENADE", 50.0),
    ("molotov", "MOLOTOV", 50.0),
    ("bat", "BASEBALLBAT", 50.0),
    ("knife", "KNIFE", 50.0),
    ("taser", "TASER", 10.0),
    ("baton", "BASEBALLBAT", 50.0),
];
/// The Playroom's additional reactions (`XR`), bit `i` of `extra_pick`.
pub const EXTRA: [&str; 8] = [
    "Windmill arms",
    "Curl up before getting up",
    "Curl up when shot at lying down",
    "Look at who hit him",
    "Tumble off a ledge",
    "smartFall when dropped",
    "Dodge from above",
    "Brace on a wall",
];
pub const GETUP_MODES: [&str; 4] = ["animation", "muscles", "off", "learned"];

/// The Playroom's switches (`settings.txt`) that its reactions read, with its defaults (`--safe`, docs/adr/0016).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// `tasks`: NM -> animation as the game's tasks end.
    pub tasks: bool,
    /// `look`, `avoid`: what's thrown at him he looks at, flinches from.
    pub look: bool,
    pub avoid: bool,
    /// `getup`: 0 animation, 1 muscles, 2 off, 3 learned.
    pub getup: u8,
    /// `extra_reactions` and `extra_pick` (the additional reactions, bit by bit).
    pub extra: bool,
    pub extra_pick: u32,
    /// `fire_reaction`: NM's OnFire when he burns.
    pub fire_react: bool,
    /// `dive_shot`: shot at again, he dives; `dodge_gap` (s) between them.
    pub dive: bool,
    pub dodge_gap: f32,
    /// `weak` (agility 2 for the weapons' sets), `god` (infinite health).
    pub weak: bool,
    pub god: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            tasks: false,
            look: true,
            avoid: false,
            getup: 0,
            extra: false,
            extra_pick: 0xff,
            fire_react: true,
            dive: false,
            dodge_gap: 3.0,
            weak: false,
            god: false,
        }
    }
}

/// One switch by its `settings.txt` name (the Playroom's `apply_settings`, for the ones its reactions read).
pub fn apply_switch(s: &mut Settings, k: &str, v: &str) -> Result<(), String> {
    let x: f32 = v.trim().parse().map_err(|_| format!("{k} {v}: not a number"))?;
    let on = x != 0.0;
    match k {
        "tasks" => s.tasks = on,
        "look" => s.look = on,
        "avoid" => s.avoid = on,
        "getup" => s.getup = (x.max(0.0) as usize % 4) as u8,
        "extra_reactions" => s.extra = on,
        "extra_pick" => s.extra_pick = x.max(0.0) as u32,
        "fire_reaction" => s.fire_react = on,
        "dive_shot" => s.dive = on,
        "dodge_gap" => s.dodge_gap = x.clamp(0.0, 10.0),
        "weak" => s.weak = on,
        "god" => s.god = on,
        _ => return Err(format!("{k}: tasks, look, avoid, getup, extra_reactions, extra_pick, fire_reaction, dive_shot, dodge_gap, weak or god")),
    }
    Ok(())
}

static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

/// The switches every unscripted Homer runs with.
pub fn settings() -> Settings {
    SETTINGS.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or_default()
}

pub fn set_settings(s: Settings) {
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
}

/// `exit`: when a game task's NM ends (min, max s, speed under which, only once balance failed).
#[derive(Clone, Copy)]
struct Exit {
    min: f32,
    max: f32,
    speed: Option<f32>,
    fail: bool,
}

const SHOT_EXIT: Exit = Exit { min: 1.0, max: 10.0, speed: Some(0.2), fail: false };

fn exit(b: &str) -> Option<Exit> {
    let e = |min, max, speed, fail| Some(Exit { min, max, speed, fail });
    match b {
        "balance" | "drunk" => e(1.0, 10.0, Some(0.2), true),
        "highfall" => e(1.0, 10.0, None, false),
        "relax" => e(1.0, 10.0, Some(0.1), false),
        "rollup" => e(2.0, 3.0, Some(0.1), false),
        "brace" => e(2.0, 30.0, None, false),
        "flinch" => e(1.0, 10.0, Some(0.2), false),
        "catchfall" | "stairs" => e(2.0, 10.0, Some(0.2), false),
        _ => None,
    }
}

/// The Playroom's `BEHAVIOURS` that a dead Homer won't do (all but `STOP all` and `relax`).
const BEHAVIOURS: [&str; 22] = [
    "balance", "drunk", "windmill", "pedal", "writhe", "rollup", "catchfall", "highfall", "stairs", "flinch", "brace", "look",
    "point", "lean", "relax", "stayup", "foetal", "grab", "overwall", "STOP all", "shot", "shot back",
];

/// The Playroom's random coin (`coin`): a fresh hash, not the game's generator.
fn coin() -> bool {
    use std::hash::BuildHasher;
    std::collections::hash_map::RandomState::new().hash_one(0u8) & 1 == 0
}

/// `Side`: the parts' places and his forward (from his thighs).
struct Side {
    parts: Vec<Vec3>,
    fwd: Vec3,
}

impl Side {
    fn new(parts: Vec<Vec3>) -> Side {
        let right = parts.get(THIGH_R).copied().unwrap_or(Vec3::X) - parts.get(THIGH_L).copied().unwrap_or(Vec3::ZERO);
        let fwd = Vec3::Z.cross(right).with_z(0.0).normalize_or(Vec3::Y);
        Side { parts, fwd }
    }
    fn p(&self, k: usize) -> Vec3 {
        self.parts.get(k).copied().unwrap_or(Vec3::ZERO)
    }
    fn left(&self) -> Vec3 {
        Vec3::Z.cross(self.fwd)
    }
}

/// Something the game did to Homer, for his next NM step (Lua's damage hooks, `dms_react`). Points and directions
/// in the game's frame (m): the follower moves them into NM's.
#[derive(Clone, Debug)]
pub enum React {
    /// A bullet: the part (or the nearest to `at`), where and which way, the Playroom's weapon, a kill.
    /// `more`: the shotgun's other pellets' parts (the Playroom's `pellets`: one shot, their damage added).
    Shot { part: Option<usize>, aim: Option<(Vec3, Vec3)>, weapon: usize, fatal: bool, more: Vec<usize> },
    /// A thrown or falling thing hit him (`obj_hit`): where, its impulse (N s). Box3D's contact already pushed him.
    Prop { at: Vec3, impulse: Vec3 },
    /// An explosion at `at`.
    Blast { at: Vec3 },
    /// He's in fire this step.
    Burn,
    /// A taser on a part.
    Taser { part: Option<usize>, at: Option<Vec3> },
    /// A blow (the Hero's `Swing`): damage, where, its impulse (N s); `head` the attacker's head.
    Melee { damage: f32, at: Vec3, impulse: Vec3, head: Vec3 },
    /// A bullet that missed him, from `from` along `dir`.
    NearMiss { from: Vec3, dir: Vec3 },
    /// A car (`id`) hit him: its impulse on him (N s) and where; `player` drives it.
    Car { id: i32, impulse: Vec3, at: Vec3, player: bool },
    /// Something thrown at him from `from` with `vel`.
    Thrown { from: Vec3, vel: Vec3 },
    /// Let go of in the air (the Playroom's `fall`).
    Dropped,
    /// The Playroom's buttons (`balance`, `flinch`, `brace`, `shot`, `RESET health`, ...).
    Button(String),
    /// Health set (`kill` = 0).
    Health(f32),
}

impl React {
    fn moved(self, o: Vec3) -> React {
        match self {
            React::Shot { part, aim, weapon, fatal, more } => React::Shot { part, aim: aim.map(|(a, d)| (a - o, d)), weapon, fatal, more },
            React::Prop { at, impulse } => React::Prop { at: at - o, impulse },
            React::Blast { at } => React::Blast { at: at - o },
            React::Taser { part, at } => React::Taser { part, at: at.map(|a| a - o) },
            React::Melee { damage, at, impulse, head } => React::Melee { damage, at: at - o, impulse, head: head - o },
            React::NearMiss { from, dir } => React::NearMiss { from: from - o, dir },
            React::Car { id, impulse, at, player } => React::Car { id, impulse, at: at - o, player },
            React::Thrown { from, vel } => React::Thrown { from: from - o, vel },
            r => r,
        }
    }
}

/// What the face and the overlay show of the last hit (`rings`): part, point on it (its frame), age (s).
#[derive(Clone, Copy, Debug, Default)]
pub struct Ring {
    pub part: usize,
    pub on: Vec3,
    pub age: f32,
}

/// The room's state for one Homer (the Playroom's `Sim` fields its reactions keep).
pub struct Room {
    pub set: Settings,
    pub health: f32,
    pub dead: bool,
    rand: Rand,
    shot_task: Option<(ShotTask, u64)>,
    blast_task: Option<delirium::explosion::Explosion>,
    on_fire: Option<nm::OnFire>,
    pub burning: f32,
    pub shock: f32,
    hurt: [f32; 4],
    near_at: f32,
    near_last: f32,
    dived: bool,
    look_stop: Option<f32>,
    lean_stop: Option<f32>,
    dodging: bool,
    dodge_at: f32,
    look_after: Option<f32>,
    fall: Option<(f32, bool)>,
    hold: Option<f32>,
    airborne: bool,
    curled: bool,
    was_fallen: bool,
    /// Getting up: the clip being played (`rises[0]`), the muscles' (`musc`), how long he's lain still, his pelvis
    /// last step; the game task's NM time (`nm_t`), its exit, its balance failed.
    pub rise: Option<Rise>,
    musc: Option<Rise>,
    still: f32,
    last: Vec3,
    nm_t: Option<f32>,
    exit: Exit,
    failed: bool,
    pub ring: Option<Ring>,
    /// The scenario's gun (`!weapon`) and its `!nododge`.
    weapon: usize,
    no_dodge: bool,
    /// The reactions waiting for the next step.
    pub queued: Vec<React>,
}

impl Room {
    pub fn new(set: Settings) -> Room {
        Room {
            set,
            health: START_HEALTH,
            dead: false,
            rand: Rand::default(),
            shot_task: None,
            blast_task: None,
            on_fire: None,
            burning: 0.0,
            shock: 0.0,
            hurt: [0.0; 4],
            near_at: -9.0,
            near_last: -9.0,
            dived: false,
            look_stop: None,
            lean_stop: None,
            dodging: false,
            dodge_at: 0.0,
            look_after: None,
            fall: None,
            hold: None,
            airborne: false,
            curled: false,
            was_fallen: false,
            rise: None,
            musc: None,
            still: 0.0,
            last: Vec3::ZERO,
            nm_t: None,
            exit: SHOT_EXIT,
            failed: false,
            ring: None,
            weapon: 0,
            no_dodge: false,
            queued: vec![],
        }
    }

    /// `xr`: additional reaction `i` is on.
    fn xr(&self, i: usize) -> bool {
        self.set.extra && self.set.extra_pick & (1 << i) != 0
    }
}

/// Which of the Playroom's weapons a name is (its own or weaponinfo.xml's), pistol for an unknown one.
pub fn weapon(name: &str) -> usize {
    WEAPONS.iter().position(|w| w.0.eq_ignore_ascii_case(name) || w.1.eq_ignore_ascii_case(name)).unwrap_or(0)
}

impl Follower {
    fn ours(&self) -> Side {
        Side::new(self.agent.doll.frames().map(|f| f.translation).collect())
    }

    fn now_s(&self) -> f32 {
        self.steps as f32 / 60.0
    }

    fn now(&self) -> u32 {
        (self.steps as f64 * 1000.0 / 60.0) as u32
    }

    fn hit(&self) -> Hit {
        Hit { health: self.room.health, ..Hit::ragdoll(&self.agent) }
    }

    fn tp(&self) -> &delirium_data::taskparams::TaskParams {
        self.tuned_tp.as_deref().unwrap_or(&self.data.tp)
    }

    /// `send_ours`: the lines to Homer.
    fn send_ours(&mut self, o: Vec<String>) {
        for l in o {
            self.send(&l);
        }
    }

    /// A reaction from the game, in the order the Playroom's would run (before the step, as its clicks do).
    pub(super) fn room_react(&mut self, r: React) {
        let r = r.moved(self.origin);
        let nearest = |s: &Follower, at: Vec3| s.agent.doll.nearest(&s.data.homer, at);
        match r {
            React::Shot { part, aim, weapon, fatal, more } => {
                let part = part.or_else(|| aim.map(|a| nearest(self, a.0))).unwrap_or(CHEST).min(PARTS - 1);
                // The Playroom's `aim`: the hit point, the bullet's way, the surface's normal (against it).
                let aim = aim.map(|(at, d)| {
                    let d = d.normalize_or(Vec3::NEG_Y);
                    (at, d, -d)
                });
                let more: Vec<usize> = more.into_iter().filter(|&k| k < PARTS).collect();
                self.fire(part, aim, fatal, &more, weapon);
            }
            React::Prop { at, impulse } => self.obj_hit(nearest(self, at), impulse),
            React::Blast { at } => self.blast_on(at),
            React::Burn => {
                if !self.room.dead {
                    self.room.burning = BURN;
                }
            }
            React::Taser { part, at } => {
                let part = part.or_else(|| at.map(|a| nearest(self, a))).unwrap_or(CHEST);
                self.zap(part);
            }
            React::Melee { damage, at, impulse, head } => self.land(damage, at, impulse, head),
            React::NearMiss { from, dir } => self.near_miss(from, dir.normalize_or(Vec3::X)),
            React::Car { id, impulse, at, player } => self.car_react(id, impulse, at, player),
            React::Thrown { from, vel } => self.throw(from, vel),
            React::Dropped => self.dropped(),
            React::Button(b) => self.room_button(&b),
            React::Health(h) => {
                self.room.health = h.max(0.0);
                if h > 0.0 {
                    self.room.dead = false;
                }
                self.say(&format!("room  health {h:.0}"));
            }
        }
    }

    /// The Playroom's scenario commands of its reactions (its `scripted`): `!blast x,y,z` (`explode`), `!shoot <part>
    /// [fatal]` (`fire` with the weapon picked), `!weapon <i>`, `!miss <x>` (`near_miss` from 6 m in front, `x` m to
    /// the side of his head), `!set <switch> <value>` (`apply_settings`), `!hfall [up]`, `!nododge`.
    pub(super) fn room_command(&mut self, c: &str, a: &str) -> Result<(), String> {
        let nums = || -> Vec<f32> { a.split(|c: char| c == ',' || c.is_whitespace()).filter_map(|x| x.trim().parse().ok()).collect() };
        match c {
            "!blast" => {
                let v = nums();
                self.blast_on(Vec3::new(v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0)));
            }
            "!shoot" => {
                let mut w = a.split_whitespace();
                let part = w.next().and_then(|k| k.parse().ok()).unwrap_or(0usize).min(PARTS - 1);
                let fatal = w.next() == Some("fatal");
                self.fire(part, None, fatal, &[], self.room.weapon);
            }
            "!weapon" => self.room.weapon = a.parse::<usize>().unwrap_or(0).min(WEAPONS.len() - 1),
            "!miss" => {
                let head = self.agent.doll.centre(HEAD);
                let from = head + Vec3::new(0.0, -6.0, 0.0);
                self.near_miss(from, (head + Vec3::X * a.parse().unwrap_or(0.5) - from).normalize());
            }
            "!set" => {
                let mut w = a.split_whitespace();
                let (k, v) = (w.next().ok_or("!set <switch> <value>")?, w.next().unwrap_or(""));
                apply_switch(&mut self.room.set, k, v)?;
            }
            "!hfall" => {
                let up = a.parse().unwrap_or(15.0);
                let mut l = vec!["stopAllBehaviours".to_string()];
                let set = delirium::highfall::Set::read(self.tp(), "*");
                l.extend(delirium::highfall::start(&set, false).iter().map(shottask::text));
                self.send_ours(l);
                self.kick_all(Vec3::Z * up);
                self.say(&format!("both  hfall: NMHFall, up {up} m/s"));
            }
            "!nododge" => self.room.no_dodge = !self.room.no_dodge,
            _ => return Err(format!("{c}: not a reaction command")),
        }
        Ok(())
    }

    /// The Playroom's `button`, for what moves Homer (not its room's things or its own toggles).
    pub(super) fn room_button(&mut self, b: &str) {
        if self.room.dead && BEHAVIOURS.contains(&b) && !matches!(b, "STOP all" | "relax") {
            return self.say(&format!("room  {b}: he's dead (health 0); Reset pose brings him back"));
        }
        self.cancel_rise();
        (self.room.still, self.room.nm_t, self.room.musc) = (0.0, None, None);
        if b == "RESET health" {
            (self.room.health, self.room.lean_stop, self.room.shot_task, self.room.dead) = (START_HEALTH, None, None, false);
            (self.room.shock, self.room.hurt, self.room.burning) = (0.0, [0.0; 4], 0.0);
            return self.say("room  reset: health 200");
        }
        if b == "shot" {
            return self.fire(CHEST, None, false, &[], 0);
        }
        if b == "shot back" {
            let s = self.ours();
            return self.fire(CHEST, Some((s.p(CHEST) - s.fwd * 0.1, s.fwd, -s.fwd)), false, &[], 0);
        }
        self.say(&format!("room  {b}"));
        if b == "lean" || b == "drunk" {
            self.room.lean_stop = Some(self.now_s() + LEAN_SECS);
        }
        let o = lines(b, &self.ours());
        if o.is_empty() {
            return self.say(&format!("room  {b}: not a button here"));
        }
        self.send_ours(o);
        if let Some(e) = exit(b).filter(|_| self.room.set.tasks) {
            (self.room.exit, self.room.nm_t, self.room.failed) = (e, Some(0.0), false);
        }
    }

    /// `fire`: a bullet into `part` (`aim` = hit point, way, normal), the weapon's damage on his health, its impulse
    /// and the ShotTask (or its next hit).
    pub(super) fn fire(&mut self, part: usize, aim: Option<(Vec3, Vec3, Vec3)>, fatal: bool, more: &[usize], w: usize) {
        self.cancel_rise();
        self.room.still = 0.0;
        let before = self.room.health;
        let was_dead = self.room.dead || before <= 0.0;
        let (label, name, fallback) = WEAPONS[w];
        let data = self.data.clone();
        let weapon = data.weapons.iter().find(|x| x.name == name);
        let hit = weapon.map(|wp| {
            use delirium_data::ped;
            let mut h = ped::Health { health: self.room.health, ..Default::default() };
            let (mut lost, mut instant) = (0.0, false);
            for &p in std::iter::once(&part).chain(more) {
                let hit = ped::Hit::new(wp.damage, wp, delirium_data::nm_asset::PART_BONES[p], ped::Attacker::Player { sure: true });
                let o = ped::damage(&mut h, &hit, &ped::Victim::default(), &mut self.room.rand);
                (lost, instant) = (lost + o.lost, instant || o.flags & ped::INSTANT != 0);
            }
            self.room.health = h.health;
            (lost, instant)
        });
        if fatal {
            self.room.health = 0.0;
        }
        if self.room.set.god {
            self.room.health = START_HEALTH;
        }
        let health = self.room.health;
        let state = if health < 50.0 { "fatally injured" } else if health < 100.0 { "injured (dying)" } else { "standing" };
        self.say(&format!(
            "room  shot ({label}) at part {part} {}: -{:.1}{} -> health {health:.1} ({state})",
            delirium_data::nm_asset::PARTS.get(part).unwrap_or(&"?"),
            hit.map_or(0.0, |h| h.0),
            if hit.is_some_and(|h| h.1) { " (instant kill)" } else { "" },
        ));
        if self.room.set.tasks {
            (self.room.exit, self.room.nm_t, self.room.failed) = (SHOT_EXIT, Some(0.0), false);
        }
        let ours = self.ours();
        let (at, d, n) = aim.unwrap_or((ours.p(part) + ours.fwd * 0.1, -ours.fwd, ours.fwd));
        self.mark(part, at);
        if let Some((lost, _)) = hit
            && !was_dead
        {
            self.injure(part, (lost / 100.0).clamp(0.15, 0.5));
        }
        let agility = if self.room.set.weak { 2 } else { 3 };
        let set = weapon.map_or("*", |wp| nm::weapon_set(wp, agility));
        let vel = shottask::bullet(at, at - d.normalize() * 3.0, weapon.map_or(fallback, |wp| wp.force));
        let mut o = vec![format!("applyImpulse partIndex={part} hitPoint={} impulse={}", v3(at), v3(vel))];
        if was_dead {
            return self.send_ours(o);
        }
        let (hit, now) = (self.hit(), self.now());
        let tp = self.tuned_tp.clone();
        let tp = tp.as_deref().unwrap_or(&data.tp);
        match self.room.shot_task.as_mut() {
            Some((t, _)) => o.extend(t.next_hit(&mut self.room.rand, tp, &hit, now, set, part, at, vel, n).iter().map(shottask::text)),
            None => self.room.shot_task = Some((ShotTask::new(&hit, set, part, at, vel, n), self.steps)),
        }
        self.send_ours(o);
    }

    /// `mark`: where the hit landed, for the face and the overlay (the Playroom's ring; its blood isn't ported).
    fn mark(&mut self, part: usize, at: Vec3) {
        if let Some(f) = self.agent.doll.frames().nth(part) {
            self.room.ring = Some(Ring { part, on: f.rotation.inverse() * (at - f.translation), age: 0.0 });
        }
    }

    /// `injure`: a limb's injury mask, worse with each hit.
    fn injure(&mut self, part: usize, by: f32) {
        let Some(l) = [(13..=16, 0), (17..=20, 1), (1..=3, 2), (4..=6, 3)].into_iter().find(|r| r.0.contains(&part)).map(|r| r.1) else { return };
        self.room.hurt[l] = (self.room.hurt[l] + by).min(INJURY_MAX);
        let line = format!("injury mask={} amount={:.2}", ["ul", "ur", "ll", "lr"][l], self.room.hurt[l]);
        self.send(&line);
    }

    /// `shot_frame` (every other step): death at health 0, the ShotTask's messages.
    fn shot_frame(&mut self, events: &mut Vec<super::Event>) {
        if self.room.health <= 0.0 && !self.room.dead {
            self.room.dead = true;
            (self.room.shot_task, self.room.blast_task) = (None, None);
            self.room_button("STOP all");
            self.room_button("relax");
            self.say("room  dead (health 0): limp; Reset pose brings him back");
            events.push(super::Event { id: self.id, kind: "died", text: String::new(), value: 0.0 });
        }
        let (mut t, made) = match self.room.shot_task.take() {
            Some((t, made)) if self.steps >= made + 2 => (t, made),
            other => return self.room.shot_task = other,
        };
        let (hit, now) = (self.hit(), self.now());
        let data = self.data.clone();
        let tp = self.tuned_tp.clone();
        let step = t.update(&mut self.room.rand, tp.as_deref().unwrap_or(&data.tp), &hit, now);
        for m in &step.msgs {
            self.send(&shottask::text(m));
        }
        if step.fire {
            self.say("ours  blind fire shot (no gun in the room)");
        }
        if step.done {
            self.say("room  the shot task finished");
        } else {
            self.room.shot_task = Some((t, made));
        }
    }

    /// `zap`: electrocute (2.5 s, renewed by each hit).
    fn zap(&mut self, part: usize) {
        self.mark(part, self.agent.doll.centre(part));
        if self.room.shock <= 0.0 {
            self.send("electrocute start=true stunMag=0.4 largeMult=2");
            self.say("room  him tased");
        }
        self.room.shock = SHOCK;
    }

    /// `burn` (every step): the taser's end, fire's damage and NM's OnFire task.
    fn burn(&mut self, dt: f32) {
        if self.room.shock > 0.0 {
            self.room.shock -= dt;
            if self.room.shock <= 0.0 {
                self.send("electrocute start=false");
            }
        }
        if self.room.dead {
            (self.room.burning, self.room.on_fire) = (0.0, None);
        }
        if self.room.burning <= 0.0 && self.room.on_fire.is_none() {
            return;
        }
        self.room.burning -= dt;
        let burning = self.room.burning > 0.0;
        if burning {
            self.room.health = if self.room.set.god { START_HEALTH } else { (self.room.health - BURN_DAMAGE * dt).max(0.0) };
        }
        let data = self.data.clone();
        let tp = self.tuned_tp.clone();
        if burning && self.room.on_fire.is_none() && self.room.set.fire_react {
            let mut f = nm::OnFire::new("*");
            f.start(&mut self.agent, &data.homer, &self.world, tp.as_deref().unwrap_or(&data.tp), &mut self.room.rand);
            self.room.on_fire = Some(f);
            self.say("room  Homer catches fire");
        }
        let failed = self.agent.feedback.iter().any(|f| f.slot == 8 && f.name == "balance");
        let health = self.room.health;
        if let Some(f) = self.room.on_fire.as_mut() {
            if failed {
                f.balance_failed();
            }
            if f.update(&mut self.agent, &data.homer, &self.world, &mut self.room.rand, dt, burning, health, health <= 0.0).is_some() {
                self.room.on_fire = None;
            }
        }
    }

    /// `blast_on`: an explosion's push on each part within 5 m, its damage, the explosion task.
    fn blast_on(&mut self, at: Vec3) {
        let ours = self.ours();
        let near = (0..PARTS).map(|k| ours.p(k).distance(at)).fold(f32::MAX, f32::min);
        if near >= BLAST_R {
            return self.say(&format!("room  blast: {near:.1} m from him"));
        }
        let mut o = vec![];
        let mut hit = vec![];
        for k in 0..PARTS {
            let p = ours.p(k);
            let f = 1.0 - p.distance(at) / BLAST_R;
            if f <= 0.0 || self.agent.doll.gone >> k & 1 != 0 {
                continue;
            }
            let imp = ((p - at).normalize_or(Vec3::Z) + Vec3::Z * 0.5).normalize() * self.agent.doll.part_mass(k) * BLAST_DV * f;
            hit.push((k, p, f));
            o.push(format!("applyImpulse partIndex={k} hitPoint={} impulse={}", v3(p), v3(imp)));
        }
        let lost = BLAST_DAMAGE * (1.0 - near / BLAST_R);
        if let Some(&(k, p, _)) = hit.iter().max_by(|a, b| a.2.total_cmp(&b.2)) {
            self.mark(k, p);
        }
        self.room.health = if self.room.set.god { START_HEALTH } else { (self.room.health - lost).max(0.0) };
        self.say(&format!("room  blast: {near:.1} m from him: -{lost:.0} -> health {:.0}", self.room.health));
        self.room.shot_task = None;
        self.room.blast_task = Some(Default::default());
        self.send_ours(o);
    }

    /// `blast_frame` (every other step): the explosion task's messages for 6 s.
    fn blast_frame(&mut self) {
        let Some(mut t) = self.room.blast_task.take() else { return };
        let set = delirium::explosion::Set::read(self.tp());
        let pel = self.ours().p(0);
        let (vz, near) = (self.agent.doll.vel(0).0.z, self.world.contact(pel, Vec3::NEG_Z, 2.0).is_some());
        let now = self.now();
        let m: Vec<String> = t.update(&set, &mut self.room.rand, now, vz, near).iter().map(shottask::text).collect();
        if !m.is_empty() {
            self.send_ours(m);
        }
        if now < t.started.unwrap_or(now) + 6000 {
            self.room.blast_task = Some(t);
        }
    }

    /// `land`, a blow (`hero::Hit::Swing`): its damage (never below 1), the game's melee task (flinch, or balance one
    /// time in 0.3), its impulse.
    fn land(&mut self, dmg: f32, at: Vec3, j: Vec3, head: Vec3) {
        let pelvis = self.agent.doll.frames().next().map_or(Vec3::ZERO, |f| f.translation);
        let ours = self.ours();
        let now = self.now_s();
        self.room.health = if self.room.set.god { START_HEALTH } else { (self.room.health - dmg).max(1.0) };
        let k = self.agent.doll.nearest(&self.data.homer, at);
        self.mark(k, at);
        let task = if self.room.rand.float_in_range(0.0, 1.0) > 0.7 { "balance" } else { "flinch" };
        let mut o = vec![format!("applyImpulse start=true partIndex={k} hitPoint={} impulse={}", v3(at), v3(j))];
        let set = match task {
            "flinch" if self.data.agility < 3 => "MELEE_PASIVE",
            "flinch" => "MELEE",
            _ => "*",
        };
        let data = self.data.clone();
        let tp = self.tuned_tp.clone();
        let (mo, until) = hero_task(tp.as_deref().unwrap_or(&data.tp), task, set, head, pelvis, ours.fwd, &mut self.room.rand);
        o.extend(mo);
        if until != 0 {
            self.room.lean_stop = Some(now + until as f32 * 1e-3);
        }
        self.send_ours(o);
        if self.room.set.tasks {
            (self.room.exit, self.room.nm_t, self.room.failed) = (exit(task).unwrap_or(SHOT_EXIT), Some(0.0), false);
        }
        if task == "flinch" {
            self.extra_look(head);
        }
        self.say(&format!("room  hit: {task} (health {:.0})", self.room.health));
    }

    /// `near_miss`: a bullet within 1.5 m of his head or chest: flinch and look (or, down, curl up; shot at again
    /// with `dive_shot`, a dive).
    fn near_miss(&mut self, from: Vec3, dir: Vec3) {
        let ours = self.ours();
        let by = |p: Vec3| from + dir * (p - from).dot(dir).max(0.0);
        let (h, c) = (by(ours.p(HEAD)), by(ours.p(CHEST)));
        let (dh, dc) = (h.distance(ours.p(HEAD)), c.distance(ours.p(CHEST)));
        let (p, d) = if dh < dc { (h, dh) } else { (c, dc) };
        if d > WHIZZ {
            return self.say(&format!("room  shot: missed by {d:.1} m (too far to notice)"));
        }
        let now = self.now_s();
        if now - std::mem::replace(&mut self.room.near_last, now) >= self.room.set.dodge_gap {
            self.room.dived = false;
        }
        let standing = {
            let fr: Vec<Transform> = self.agent.doll.frames().collect();
            fr[HEAD].translation.z > 1.4 && fr[0].translation.z > 0.8
        };
        if self.room.dead || !self.agent.balancing() && !standing {
            if !self.room.dead && self.room.xr(2) {
                if !self.room.curled {
                    self.room.curled = true;
                    self.send("bodyFoetal start=true");
                }
                self.room.still = 0.0;
                self.send(&format!("headLook start=true pos={} alwaysLook=true", v3(p)));
                return self.say(&format!("room  shot: near miss, {d:.2} m from him (down: curls up, additional reactions)"));
            }
            return self.say(&format!("room  shot: near miss, {d:.2} m from him (down: no reaction)"));
        }
        self.say(&format!("room  shot: near miss, {d:.2} m from him"));
        let since = now - self.room.near_at;
        if since >= self.room.set.dodge_gap || since < 0.0 {
            self.room.near_at = now;
        }
        if self.room.set.dive && (NEAR_BURST..self.room.set.dodge_gap).contains(&since) && !self.room.no_dodge && !self.room.dived {
            (self.room.near_at, self.room.dived) = (now, true);
            self.say("room  shot at again: he dives");
            let chest = ours.p(CHEST);
            let off = (chest - by(chest)).with_z(0.0);
            let away = if off.length() > 0.05 { off.normalize() } else { dir.cross(Vec3::Z).normalize_or(Vec3::X) };
            // The Playroom jumps a balancing Homer (its `jump`, not ported): he's thrown as it throws one down.
            self.kick_all(away * DIVE.0 + Vec3::Z * DIVE.1);
            return self.send_ours(vec![format!("shootDodge start=true rightArmTarget={} leftArmTarget={}", v3(from), v3(from))]);
        }
        let o = vec![
            "bodyBalance start=true".into(),
            format!("upperBodyFlinch start=true pos={} newHit=true", v3(p)),
            format!("headLook start=true pos={} alwaysLook=true", v3(p)),
        ];
        self.send_ours(o);
        if let Some(e) = exit("flinch").filter(|_| self.room.set.tasks) {
            (self.room.exit, self.room.nm_t, self.room.failed) = (e, Some(0.0), false);
        }
    }

    /// `car_react`: the game's bump damage of a car's hit and the task it picks.
    fn car_react(&mut self, k: i32, sum: Vec3, at: Vec3, player: bool) {
        use delirium::bump::{self, Kind, Thing};
        let a = &self.agent;
        let other = Thing { id: k, kind: Kind::Vehicle { player_driver: player } };
        let imp = bump::Impact { impulse: sum.length(), dt: 1.0 / 60.0, component: a.doll.nearest(&self.data.homer, at) as i32, other: Some(other) };
        let health = self.room.health;
        let ped = bump::Ped {
            id: -2,
            health,
            inv_mass: 1.0 / a.doll.mass().max(1.0),
            woke: false,
            drop: None,
            vz: a.doll.vel(0).0.z,
            tough: false,
            player_cut: false,
            player: false,
            mission: false,
            quick_relax: false,
            agility: self.data.agility,
            gun: false,
            place: a.doll.frames().next().map_or(at, |f| f.translation),
            vel: Vec3::ZERO,
            standing_on: None,
        };
        let Some((amount, _)) = bump::damage(&ped, &imp, &mut self.room.rand).filter(|d| d.0 >= 1.0) else { return };
        let left = if self.room.set.god { START_HEALTH } else { (health - amount).max(1.0) };
        self.room.health = left;
        let part = self.agent.doll.nearest(&self.data.homer, at);
        self.injure(part, (amount / 100.0).clamp(0.1, 0.5));
        let around = bump::Around {
            touching: vec![other],
            player: player.then_some(bump::Player { thing: other, place: at, in_car: true }),
            blend: None,
            sit: None,
            die: None,
            accepts: true,
        };
        let task = match bump::react(&ped, &imp, &around, amount, false, left, &mut self.room.rand, || None).map(|m| m.task) {
            Some(bump::Task::Brace { .. }) => "brace",
            Some(bump::Task::RollUp { .. }) => "rollup",
            Some(bump::Task::Balance { .. }) => "balance",
            Some(_) => "relax",
            None => "",
        };
        self.say(&format!(
            "room  a car hits him: {amount:.0} damage (health {left:.0}){}",
            if task.is_empty() { String::new() } else { format!(", {task}") }
        ));
        if !task.is_empty() {
            self.room_button(task);
        }
    }

    /// `throw`'s messages: he looks at what's thrown at him (`look`), flinches from it (`avoid`).
    fn throw(&mut self, from: Vec3, vel: Vec3) {
        if self.room.dead {
            return;
        }
        let mut m = vec![];
        if self.room.set.look {
            m.push(format!("headLook start=true pos={} vel={} instanceIndex=-1 alwaysLook=true", v3(from), v3(vel)));
        }
        if self.room.set.avoid {
            m.push(format!("upperBodyFlinch start=true pos={} newHit=true", v3(from)));
            (self.room.dodging, self.room.look_stop) = (true, Some(self.now_s() + 2.0));
        }
        self.send_ours(m);
    }

    /// `obj_hit`'s part on Homer: a hit over 1 N s costs 0.6 health a N s (1 to 35, never below 1) and bruises the
    /// limb. Its `applyImpulse` isn't sent: Box3D's contact already pushed the body (as a bullet's, ROADMAP M6).
    fn obj_hit(&mut self, k: usize, imp: Vec3) {
        if imp.length() <= 1.0 {
            return;
        }
        let amount = (imp.length() * 0.6).clamp(1.0, 35.0);
        self.say(&format!("ours  a thing hits {} ({:.1} N s)", delirium_data::nm_asset::PARTS.get(k).unwrap_or(&"?"), imp.length()));
        if !self.room.set.god {
            self.room.health = (self.room.health - amount).max(1.0);
        }
        self.injure(k, (amount / 200.0).clamp(0.05, 0.2));
    }

    /// `fall`'s reaction: dropped, a smartFall (additional reactions).
    fn dropped(&mut self) {
        if self.room.xr(5) {
            self.send("smartFall start=true");
            self.say("ours  dropped: smartFall (additional reactions)");
        }
    }

    /// `extra_look`: he looks at who hit him for 2 s (additional reactions).
    fn extra_look(&mut self, at: Vec3) {
        if self.room.xr(3) && !self.room.dead {
            self.send(&format!("headLook start=true pos={} alwaysLook=true", v3(at)));
            self.room.look_stop = Some(self.now_s() + 2.0);
        }
    }

    /// The Playroom's `in_air` (every other step): falling faster than 6 m/s with no extra running, `stopAllBehaviours`
    /// and the game's NMHighFall task.
    pub(super) fn in_air(&mut self, com_v: Vec3) {
        if self.room.xr(4) || self.room.dead || self.room.nm_t.is_some() || !self.agent.extras.on.is_empty() || self.agent.high_falling() || com_v.z > -6.0 {
            return;
        }
        self.send("stopAllBehaviours");
        let data = self.data.clone();
        let tp = self.tuned_tp.clone();
        nm::task_high_fall(&mut self.agent, &data.homer, &self.world, tp.as_deref().unwrap_or(&data.tp), "*", false);
        if let Some(e) = exit("highfall").filter(|_| self.room.set.tasks) {
            (self.room.exit, self.room.nm_t, self.room.failed) = (e, Some(0.0), false);
        }
        self.say("ours  in the air: the game's NMHighFall");
    }

    /// `kick`: the same velocity onto every part (Box3D's bodies: an impulse each).
    fn kick_all(&mut self, v: Vec3) {
        for k in 0..PARTS {
            self.kicks.push((k, Vec3::ZERO, v * self.agent.doll.part_mass(k)));
        }
    }

    /// `self_preserve` (every other step), the additional reactions that read the world: off a ledge he tumbles or
    /// drops arms out, into a wall he braces on it, he dodges what falls on him or comes at him (the props near him,
    /// NM's obstacles).
    fn self_preserve(&mut self) {
        const DODGE_DUE: f32 = 1.0;
        const BRACE: f32 = 0.45;
        const ARMS_OUT: &str = "bodyBalance start=true armsOutOnPush=true armsOutOnPushMultiplier=1 armsOutOnPushTimeout=1.1";
        let now = self.now_s();
        let r = &self.room;
        if r.dead || !(r.xr(4) || r.xr(6) || r.xr(7)) && r.fall.is_none() && r.look_after.is_none() && r.hold.is_none() && !r.airborne {
            return;
        }
        let (me, mv) = self.agent.doll.com();
        let things: Vec<(Vec3, Vec3, bool)> = self.obstacles.iter().map(|o| (o.centre, o.vel, true)).collect();
        let overhead = || {
            things
                .iter()
                .filter_map(|&(p, v, _)| {
                    let (r, down) = (p - me, -v.z);
                    let h = r.z - 0.8;
                    (h > 0.0 && r.with_z(0.0).length() < 2.5 && down > 1.0).then(|| (p, v, (-down + (down * down + 2.0 * 9.81 * h).sqrt()) / 9.81))
                })
                .filter(|x| x.2 < 3.0)
                .min_by(|a, b| a.2.total_cmp(&b.2))
        };
        if self.room.look_after.is_some_and(|t| now >= t) {
            self.room.look_after = None;
            if std::mem::take(&mut self.room.dodging) {
                self.send("stopAllBehaviours");
                self.send(ARMS_OUT);
            }
            let down = things.iter().map(|x| x.0).filter(|p| (*p - me).length() < 4.0).min_by(|a, b| (*a - me).length().total_cmp(&(*b - me).length()));
            if let Some(at) = down {
                self.extra_look(at);
                self.room.look_stop = Some(now + 2.5);
            }
        }
        let world = self.world.clone();
        let cast = move |from: Vec3, dir: Vec3, len: f32| world.cast(from, dir, len);
        let land = cast(me, -Vec3::Z, 50.0).map_or(f32::MAX, |(t, _)| (t - 0.95) / (-mv.z).max(0.1));
        if self.room.airborne {
            if land < 0.12 || mv.z > 0.0 {
                self.room.airborne = false;
                self.send("bodyBalance start=true");
            }
        } else if self.room.xr(4) && mv.z < -0.8 && land > 0.4 && self.agent.balancing() {
            if mv.truncate().length() > 0.8 && land > 0.8 {
                self.send("stopAllBehaviours");
                self.send(&format!("smartFall start=true tumble={TUMBLE}"));
                self.say("ours  over the edge: tumbling (additional reactions)");
            } else {
                self.room.airborne = true;
                self.send("highFall start=true");
                self.say("ours  dropping: arms out (additional reactions)");
            }
        }
        if self.room.hold.is_some_and(|t| now >= t) {
            self.room.hold = None;
            self.send("grab start=false");
        }
        let (go, chest) = (mv.with_z(0.0), self.ours().p(CHEST));
        if self.room.xr(7) && self.room.hold.is_none() && self.room.fall.is_none() && go.length() > 0.4 && chest.z - me.z > 0.2 {
            let d = go.normalize();
            let reach = 0.35 + (0.3 * go.length()).min(0.7);
            let hit = [chest.z, me.z, me.z - 0.3].into_iter().find_map(|z| {
                let from = me.with_z(z);
                cast(from, d, reach).filter(|h| h.1.z.abs() < 0.5).map(|(t, n)| (from + d * t, n.with_z(0.0).normalize()))
            });
            if let Some((h, n)) = hit {
                let head = chest.z + 0.45;
                let right = (-n).cross(Vec3::Z);
                let top = (h - n * 0.03).with_z(head);
                let top = top.with_z(head - cast(top, -Vec3::Z, 1.5).map_or(0.0, |x| x.0));
                let l = if cast(me.with_z(head), d, reach + 0.2).is_some() {
                    let at = h.with_z(chest.z);
                    format!(
                        "grab start=true justbrace=true useLeft=true useRight=true pos1={} pos2={} normalR={} normalL={}",
                        v3(at + right * 0.25),
                        v3(at - right * 0.25),
                        v3(n),
                        v3(n)
                    )
                } else if top.z > me.z - 0.15 {
                    format!("grab start=true linegrab=true useLeft=true useRight=true pos1={} pos2={}", v3(top + right * 0.3), v3(top - right * 0.3))
                } else {
                    String::new()
                };
                if !l.is_empty() {
                    self.room.hold = Some(now + 2.0);
                    self.send(&l);
                    self.say(&format!("ours  into a wall/rail: {} (additional reactions)", if l.contains("justbrace") { "hands on it" } else { "grabs it" }));
                }
            }
        }
        if let Some((land, braced)) = self.room.fall {
            let seen = overhead();
            if seen.is_none() && now > land + 0.3 || now > land + 1.5 {
                (self.room.fall, self.room.look_after) = (None, Some(now + 0.4));
                return;
            }
            let Some((at, _, t)) = seen else { return };
            self.room.fall = Some((now + t, braced || t < BRACE));
            if !braced && t < BRACE {
                let away = (me - at).with_z(0.0).normalize_or(-self.ours().fwd.with_z(0.0));
                self.room.dodging = true;
                self.room.lean_stop = Some(now + 0.8);
                for l in [
                    "pointArm start=false".to_string(),
                    ARMS_OUT.to_string(),
                    format!("upperBodyFlinch start=true pos={} newHit=true", v3(at)),
                    format!("leanInDirection start=true applyAsForce=true leanAmount=0.7 dir={}", v3(away)),
                ] {
                    self.send(&l);
                }
            } else if !braced && self.steps % 10 == 0 {
                self.send(&format!("pointArm start=true target={}", v3(at)));
            }
            return;
        }
        if !self.room.xr(6) || now < self.room.dodge_at || !self.agent.balancing() {
            return;
        }
        if let Some((at, v, t)) = overhead() {
            self.room.dodge_at = now + 3.0;
            if (at - me).with_z(0.0).dot(self.ours().fwd) < -0.2 {
                self.room.look_after = Some(now + t + 0.7);
                self.send(ARMS_OUT);
                return self.say("ours  something falling on him from behind: arms out (additional reactions)");
            }
            self.room.fall = Some((now + t, false));
            self.room.look_stop = None;
            for l in [format!("headLook start=true pos={} vel={} alwaysLook=true", v3(at), v3(v)), format!("pointArm start=true target={}", v3(at))] {
                self.send(&l);
            }
            return self.say("ours  something falling on him: watches it, arm up (additional reactions)");
        }
        let due = |p: Vec3, v: Vec3| (me - p).length() / (me - p).normalize_or_zero().dot(v - mv).max(1e-3);
        let Some(&(at, v, thrown)) = things.iter().find(|&&(p, v, _)| (me - p).length() < 30.0 && (me - p).normalize_or_zero().dot(v - mv) > 3.0 && due(p, v) < DODGE_DUE)
        else {
            return;
        };
        let away = (me - at).with_z(0.0).normalize_or(-self.ours().fwd.with_z(0.0));
        self.room.dodge_at = now + 3.0;
        self.room.look_stop = Some(now + 2.0);
        self.room.lean_stop = Some(now + DODGE_DUE);
        let lean = format!("leanInDirection start=true applyAsForce=true leanAmount=0.6 dir={}", v3(away));
        let look = format!("headLook start=true pos={} vel={} alwaysLook=true", v3(at), v3(v));
        if thrown {
            self.room.dodging = true;
            for l in [look, format!("upperBodyFlinch start=true pos={} newHit=true", v3(at)), lean] {
                self.send(&l);
            }
        } else {
            for l in [ARMS_OUT.to_string(), look, lean] {
                self.send(&l);
            }
        }
        self.say("ours  something coming: dodging (additional reactions)");
    }

    /// The room's part of a step before it (the Playroom's `tick` up to its step): fire every step; every other, the
    /// shot and explosion tasks, `in_air`, the additional reactions.
    pub(super) fn room_before(&mut self, com_v: Vec3, events: &mut Vec<super::Event>) {
        for r in std::mem::take(&mut self.room.queued) {
            self.room_react(r);
        }
        if let Some(r) = self.room.ring.as_mut() {
            r.age += 1.0 / 60.0;
        }
        self.burn(1.0 / 60.0);
        if self.steps % 2 == 0 {
            self.shot_frame(events);
            self.blast_frame();
            self.in_air(com_v);
            self.self_preserve();
        }
    }

    /// The room's part after the step (the end of `tick`): the look and lean that run out, a failed balance.
    pub(super) fn room_after(&mut self) {
        let now = self.now_s();
        if self.room.look_stop.is_some_and(|t| now >= t) {
            self.room.look_stop = None;
            self.send("headLook start=false");
            if std::mem::take(&mut self.room.dodging) && self.agent.balancing() {
                self.send("stopAllBehaviours");
                self.send("bodyBalance start=true");
            }
        }
        if self.room.lean_stop.is_some_and(|t| now >= t) {
            self.room.lean_stop = None;
            self.say("room  lean stops");
            self.send("leanInDirection start=false");
        }
        self.room.failed |= self.agent.feedback.iter().any(|f| f.slot == 8 && f.name == "balance");
    }

    /// Off the clip: the bodies go back to NM (Lua unfreezes them, then `DMS.FollowRiseDone`).
    fn cancel_rise(&mut self) {
        if self.room.rise.take().is_some() {
            self.unfreeze = true;
        }
    }

    /// The Playroom's `rise` (after each step): windmilling as he falls (additional reactions); the get-up clip
    /// playing on; a game task's NM time out (tasks on); lying still 2 s with health >= 50, the get-up of the mode.
    pub(super) fn rise_step(&mut self, dt: f32) {
        let fallen = self.agent.fallen;
        if self.room.xr(0) && fallen && !std::mem::replace(&mut self.room.was_fallen, fallen) && coin() {
            self.send("armsWindmillAdaptive start=true disableOnImpact=true");
            self.say("ours  off balance: windmilling (additional reactions)");
        }
        self.room.was_fallen = fallen;
        let data = self.data.clone();
        let Ok(clips) = data.clips.as_ref() else { return };
        let now: Vec<Transform> = self.agent.doll.frames().collect();
        let pelvis = now[0].translation;
        if self.room.musc.is_some() {
            self.musc_rise(clips, dt);
            self.room.last = pelvis;
            return;
        }
        if let Some(r) = self.room.rise.as_mut() {
            r.t += dt;
            if clips.frames(r).is_none() {
                let end = clips.end(r);
                self.room.rise = None;
                if self.room.set.tasks {
                    self.room.rise = Some(clips.stand(end));
                    return;
                }
                // A new agent on the clip's last pose, balancing: the bodies are there (Lua held them on the clip).
                self.respawn = Some(vec!["bodyBalance start=true".into()]);
                self.unfreeze = true;
                self.say("ours  up: balancing");
            }
            return;
        }
        if let Some(t) = self.room.nm_t.map(|t| t + dt) {
            self.room.nm_t = Some(t);
            let (e, lying) = (self.room.exit, now[HEAD].translation.z - pelvis.z < 0.3);
            let v = self.agent.doll.com().1.length();
            let slow = e.speed.filter(|_| !e.fail || self.room.failed).is_some_and(|s| v < s);
            if (t >= e.min && slow) || (t >= e.max && v < 2.0) {
                self.say(&format!("ours  game task: out of NM after {t:.2} s at {v:.2} m/s: {}", if lying { "getting up" } else { "back to the animation" }));
                self.room.nm_t = None;
                self.room.rise = Some(if lying { clips.start(now) } else { clips.stand(now) });
                self.freeze = true;
            }
            self.room.last = pelvis;
            return;
        }
        let down = self.room.set.getup != 2
            && !self.room.set.tasks
            && self.room.health >= 50.0
            && now[HEAD].translation.z - pelvis.z < 0.3
            && pelvis.distance(self.room.last) < 0.2 * dt.max(1e-3);
        self.room.still = if down { self.room.still + dt } else { 0.0 };
        if self.room.still >= getup::DOWN_SECS + if self.room.curled { 3.0 } else { 0.0 } {
            self.room.still = 0.0;
            if self.room.xr(1) && !self.room.curled && coin() {
                self.room.curled = true;
                self.send("bodyFoetal start=true");
                self.say("ours  curled up (additional reactions)");
                self.room.last = pelvis;
                return;
            }
            if std::mem::take(&mut self.room.curled) {
                self.send("bodyFoetal start=false");
            }
            match self.room.set.getup {
                3 => self.say("ours  no getup.net here (the learned get-up isn't ported)"),
                1 => {
                    self.room.musc = Some(clips.start(now));
                    nm::task_pose(&mut self.agent, &data.homer, &self.world, &mut self.room.rand, MUSC_STIFF.0, MUSC_STIFF.1);
                    self.say("ours  getting up with his muscles (the clip's poses as NMPose's; he can fail)");
                }
                _ => {
                    self.room.rise = Some(clips.start(now));
                    self.freeze = true;
                    self.say("ours  getting up (the game's get_up_fast; the viewer's, not the game's task)");
                }
            }
        }
        self.room.last = pelvis;
    }

    /// `musc_rise`: the clip's poses as NMPose's targets, a hand at the pelvis toward the clip's (at most half his
    /// weight).
    fn musc_rise(&mut self, clips: &getup::Clips, dt: f32) {
        let Some(r) = self.room.musc.as_mut() else { return };
        r.t += dt;
        match clips.frames(r) {
            Some(p) => {
                let (at, v) = (self.agent.doll.frames().next().map_or(Vec3::ZERO, |f| f.translation), self.agent.doll.com().1);
                let m = self.agent.doll.mass();
                let f = (m * (40.0 * (p[0].translation - at) - 8.0 * v)).clamp_length_max(MUSC_HELP * m * 9.81);
                self.agent.push.push((0, at, f));
                nm::task_pose_control(&mut self.agent, p)
            }
            None => {
                self.room.musc = None;
                self.send("bodyBalance start=true");
                self.say("ours  muscles' get-up done: balancing");
            }
        }
    }

    /// The clip's pose now while he gets up by animation (NM's frame), for Lua to hold the bodies on.
    pub fn rise_pose(&self) -> Option<Vec<Transform>> {
        let r = self.room.rise.as_ref()?;
        self.data.clips.as_ref().ok()?.frames(r)
    }
}

/// The Playroom's `lines`: a button's messages.
fn lines(b: &str, s: &Side) -> Vec<String> {
    let (head, chest) = (s.p(HEAD), s.p(CHEST));
    match b {
        "balance" => vec!["bodyBalance start=true".into()],
        "drunk" => vec!["bodyBalance start=true armsOutOnPushTimeout=999".into(), format!("leanInDirection start=true leanAmount=0.2 dir={}", v3(s.fwd))],
        "windmill" => vec!["bodyBalance start=true".into(), "armsWindmillAdaptive start=true".into()],
        "pedal" => vec!["pedalLegs start=true".into()],
        "writhe" => vec!["bodyWrithe start=true".into()],
        "rollup" => vec!["bodyRollUp start=true".into()],
        "catchfall" => vec!["catchFall start=true".into()],
        "highfall" => vec!["highFall start=true".into()],
        "stairs" => vec!["rollDownStairs start=true".into()],
        "flinch" => vec![format!("upperBodyFlinch start=true pos={}", v3(head + s.fwd * 2.0))],
        "brace" => vec![format!("braceForImpact start=true pos={} look={}", v3(chest + s.fwd), v3(chest + s.fwd))],
        "look" => vec![format!("headLook start=true alwaysLook=true pos={}", v3(head + s.fwd * 2.0 + s.left()))],
        "point" => vec![format!("pointArm start=true target={}", v3(chest + s.fwd * 2.0 - s.left()))],
        "lean" => vec!["bodyBalance start=true".into(), format!("leanInDirection start=true leanAmount=0.4 dir={}", v3(s.fwd))],
        "relax" => vec!["bodyRelax start=true".into()],
        "stayup" => vec!["stayUpright start=true stiffness=3".into()],
        "foetal" => vec!["bodyFoetal start=true".into()],
        "grab" => {
            let at = chest + s.fwd * 0.45;
            vec![format!(
                "grab start=true linegrab=true useLeft=true useRight=true dontLetGo=true pos1={} pos2={}",
                v3(at + s.left() * 0.5),
                v3(at - s.left() * 0.5)
            )]
        }
        "overwall" => vec![format!("fallOverWall start=true fallOverPoint={} fallOverEdge={}", v3((s.p(0) - s.fwd * 0.3).with_z(0.9)), v3(s.left()))],
        "STOP all" => vec!["stopAllBehaviours".into()],
        x if delirium::extras::messages().any(|m| m.name == x) => vec![format!("{x} start=true")],
        _ => vec![],
    }
}

/// `hero_task`: the game's melee task (flinch or balance) for a blow, its first messages and how long it leans (ms).
fn hero_task(tp: &delirium_data::taskparams::TaskParams, task: &str, set: &str, head: Vec3, me: Vec3, fwd: Vec3, rand: &mut Rand) -> (Vec<String>, u32) {
    use delirium::meleetask::MeleeTask;
    let hit = Hit { attacker_head: Some(head), attacker_neck: Some(head), pos: me, fwd, ..Default::default() };
    let mut t = if task == "flinch" { MeleeTask::Flinch { set: set.into(), started: false, lean_until: 0 } } else { MeleeTask::Balance { set: set.into(), started: false } };
    let lines = t.update(rand, tp, &hit, 0, false).iter().map(shottask::text).collect();
    (lines, if let MeleeTask::Flinch { lean_until, .. } = t { lean_until } else { 0 })
}

/// The Playroom's buttons a reaction can press (`dms_react button <name>`, the menu).
pub const BUTTONS: [&str; 22] = [
    "balance", "drunk", "windmill", "pedal", "writhe", "rollup", "catchfall", "highfall", "stairs", "flinch", "brace", "look",
    "point", "lean", "relax", "stayup", "foetal", "grab", "overwall", "STOP all", "shot", "shot back",
];
