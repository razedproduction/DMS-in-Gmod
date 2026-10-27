//! The Playroom's get-up clips (`delirium-room` `getup::Clips`): the game's `get_up_fast` (from the front and from
//! the back) and `move_player`'s `idle`, on the player's skeleton, placed on Homer's parts. Only what its `rise` uses:
//! `start`, `stand`, `frames`, `end` (ROADMAP M6).

use delirium::prelude::*;
use delirium::ragdoll::Build;
use delirium_data::{anim, model, resource, rpf, skeleton};

use crate::{Game, Platform};

/// `getup::DOWN_SECS`: how long he lies still before he gets up.
pub const DOWN_SECS: f32 = 2.0;
const RISE_BLEND: f32 = 0.25;
/// `getup::IDLE`: the clips before it are the get-ups.
pub const IDLE: usize = 2;

/// `getup::Clips`, the part `rise` plays.
#[derive(Clone)]
pub struct Clips {
    parents: Vec<Option<usize>>,
    bind: Vec<(Vec3, Quat)>,
    clips: Vec<(anim::Animation, Vec<Option<usize>>)>,
    parts: Vec<(usize, Transform)>,
    chest: usize,
}

/// `getup::Rise`: a clip being played from where he lay.
#[derive(Clone)]
pub struct Rise {
    clip: usize,
    hold: bool,
    pub t: f32,
    body: Transform,
    lay: Vec<Transform>,
}

impl Game {
    /// `Clips::load`'s get-up and idle clips for `build`'s parts.
    pub fn clips(&self, build: &Build) -> Result<Clips, String> {
        let peds = match self.platform {
            Platform::Pc => {
                let p = self.dir.join("pc/models/cdimages/playerped.rpf");
                std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?
            }
            Platform::Xbox360 => {
                let p = self.dir.join("xbox360.rpf");
                let platform = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                let toc = rpf::read_toc(&platform, &self.key())?;
                let nested = toc.iter().find(|e| e.path.eq_ignore_ascii_case("models/cdimages/playerped.rpf")).ok_or("playerped.rpf not found")?;
                rpf::read(&platform, nested)?
            }
        };
        let inner = rpf::read_toc(&peds, &self.key())?;
        let e = inner.iter().find(|e| e.path.eq_ignore_ascii_case(&self.plat("player.xft"))).ok_or("player.xft not in playerped.rpf")?;
        let frag = resource::parse(&rpf::read(&peds, e)?)?;
        let bones = skeleton::read(&frag, model::fragment_root(&frag)?)?;
        let (table, mut f) = self.img_open("xbox360/anim/anim.img")?;
        let mut clip = |dict: &str, name: &str| -> Result<(anim::Animation, Vec<Option<usize>>), String> {
            let a = anim::read_dictionary(&self.img_read(&table, &mut f, dict)?)?
                .into_iter()
                .find(|a| a.name.eq_ignore_ascii_case(name))
                .ok_or(format!("no {name} in {dict}"))?;
            let map = a.tracks.iter().map(|t| bones.iter().position(|b| b.id == t.bone).filter(|&b| b != 0)).collect();
            Ok((a, map))
        };
        let clips = vec![clip("get_up.xad", "get_up_fast")?, clip("get_up_back.xad", "get_up_fast")?, clip("move_player.xad", "idle")?];
        let parts = build
            .parts
            .iter()
            .map(|p| bones.iter().position(|b| b.name == p.name).map(|b| (b, p.offset)).ok_or(format!("no bone {}", p.name)))
            .collect::<Result<_, _>>()?;
        let chest = build.parts.iter().position(|d| d.parent == Some(0) && !d.lower).unwrap_or(0);
        Ok(Clips {
            parents: bones.iter().map(|b| b.parent).collect(),
            bind: bones.iter().map(|b| (Vec3::from(b.translation), Quat::from_array(b.rotation).normalize())).collect(),
            clips,
            parts,
            chest,
        })
    }
}

impl Clips {
    /// `Clips::pose`.
    fn pose(&self, c: usize, phase: f32) -> Vec<(Vec3, Quat)> {
        let (a, map) = &self.clips[c];
        let x = phase.clamp(0.0, 1.0) * (a.frames - 1) as f32;
        let (i, f) = (x.floor() as usize, x.fract());
        let q = |v: &[f32]| Quat::from_xyzw(v[0], v[1], v[2], v[3]).normalize();
        let mut out = self.bind.clone();
        out[0] = (Vec3::ZERO, Quat::IDENTITY);
        for (t, bone) in a.tracks.iter().zip(map) {
            let (va, vb) = (t.sample(i), t.sample(i + 1));
            match (t.kind, bone, t.bone) {
                (anim::BONE_ROTATION, Some(b), _) => out[*b].1 = q(&va).slerp(q(&vb), f),
                (anim::BONE_TRANSLATION, Some(b), _) => out[*b].0 = Vec3::from_slice(&va).lerp(Vec3::from_slice(&vb), f),
                (anim::BONE_ROTATION, None, 0) => out[0].1 = q(&va).slerp(q(&vb), f),
                (anim::BONE_TRANSLATION | anim::MOVER_TRANSLATION, None, 0) => out[0].0 += Vec3::from_slice(&va).lerp(Vec3::from_slice(&vb), f),
                _ => {}
            }
        }
        out
    }

    /// `Clips::place` (no `grow` scale: Homer's own body).
    fn place(&self, pose: &[(Vec3, Quat)], body: Transform) -> Vec<Transform> {
        let mut bones: Vec<(Vec3, Quat)> = Vec::with_capacity(pose.len());
        for (i, &(t, r)) in pose.iter().enumerate() {
            let (pt, pr) = self.parents[i].map_or((body.translation, body.rotation), |p| bones[p]);
            bones.push((pt + pr * t, pr * r));
        }
        self.parts
            .iter()
            .map(|&(b, o)| {
                let (bt, br) = bones[b];
                Transform::from_translation(bt + br * o.translation).with_rotation(br * o.rotation)
            })
            .collect()
    }

    /// `Clips::start`: the get-up (front or back) whose first chest is nearest how he lies.
    pub fn start(&self, lay: Vec<Transform>) -> Rise {
        let yaw = |q: Quat| {
            let a = q * Vec3::Y;
            a.y.atan2(a.x)
        };
        let ch = self.chest;
        let (_, clip, body) = (0..IDLE)
            .map(|c| {
                let first = self.place(&self.pose(c, 0.0), Transform::IDENTITY);
                let turn = Quat::from_rotation_z(yaw(lay[ch].rotation) - yaw(first[ch].rotation));
                let body = Transform::from_translation(lay[0].translation - turn * first[0].translation).with_rotation(turn);
                ((turn * first[ch].rotation).angle_between(lay[ch].rotation), c, body)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .expect("a get-up clip");
        Rise { clip, hold: false, t: 0.0, body, lay }
    }

    /// `Clips::stand`: the idle clip, held (back to the animation, standing).
    pub fn stand(&self, lay: Vec<Transform>) -> Rise {
        let yaw = |q: Quat| {
            let a = q * Vec3::Y;
            a.y.atan2(a.x)
        };
        let first = self.place(&self.pose(IDLE, 0.0), Transform::IDENTITY);
        let turn = Quat::from_rotation_z(yaw(lay[0].rotation) - yaw(first[0].rotation));
        let body = Transform::from_translation(lay[0].translation - turn * first[0].translation).with_rotation(turn);
        Rise { clip: IDLE, hold: true, t: 0.0, body, lay }
    }

    /// `Clips::frames`: the parts' frames at the rise's time, blended in from where he lay; none once it's over.
    pub fn frames(&self, r: &Rise) -> Option<Vec<Transform>> {
        let d = self.clips[r.clip].0.duration;
        (r.t <= d || r.hold).then(|| {
            let k = (r.t / RISE_BLEND).min(1.0);
            self.place(&self.pose(r.clip, if r.hold { (r.t / d).fract() } else { r.t / d }), r.body)
                .into_iter()
                .zip(&r.lay)
                .map(|(n, l)| Transform::from_translation(l.translation.lerp(n.translation, k)).with_rotation(l.rotation.slerp(n.rotation, k)))
                .collect()
        })
    }

    /// `Clips::end`: the clip's last frame.
    pub fn end(&self, r: &Rise) -> Vec<Transform> {
        self.place(&self.pose(r.clip, 1.0), r.body)
    }
}
