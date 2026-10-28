//! The reactions' natives (ROADMAP M6): Lua turns the game's damage into the Playroom room's reactions
//! (`DMS.FollowReact`), holds Homer on his get-up clip (`DMS.FollowPose`, `DMS.FollowRiseDone`), sets the Playroom's
//! switches (`DMS.SetReactions`) and the player's tuning (`DMS.SetTuning`, docs/adr/0009), and reads what the face
//! and the debug overlay draw (`DMS.FollowRoom`, `DMS.FollowOverlay`).

use std::ffi::{CStr, c_int};

use delirium::prelude::*;

use crate::data;
use crate::follow::{self, React, Settings};
use crate::lua::{Api, LuaState, native};
use crate::step;
use crate::tuning;

pub const NATIVES: &[(&CStr, crate::lua::CFunction)] = &[
    (c"FollowReact", follow_react),
    (c"FollowPose", follow_pose),
    (c"FollowRiseDone", follow_rise_done),
    (c"FollowRiseCancel", follow_rise_cancel),
    (c"FollowRoom", follow_room),
    (c"FollowOverlay", follow_overlay),
    (c"SetReactions", set_reactions),
    (c"Reactions", reactions),
    (c"SetTuning", set_tuning),
    (c"Weapons", weapons),
    (c"TaskParams", task_params),
];

native!(task_params => task_params_impl);
/// `DMS.TaskParams()`: the game's taskparams.txt as NM's tasks read it, `{[task] = {[set] = {[param] = value}}}` (the
/// first value of each; empty without the file: NM's defaults), for the tuning menu.
unsafe fn task_params_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let d = data::get().ok_or("NM's data isn't loaded (DMS.LoadData)")?;
    let cs = |s: &str| std::ffi::CString::new(s).unwrap_or_default();
    unsafe {
        (api.createtable)(l, 0, d.tp.0.len() as c_int);
        for (task, sets) in &d.tp.0 {
            (api.createtable)(l, 0, sets.len() as c_int);
            for (set, params) in sets {
                (api.createtable)(l, 0, params.len() as c_int);
                for (p, v) in params {
                    if let Some(x) = v.first() {
                        (api.pushnumber)(l, f64::from(*x));
                        (api.setfield)(l, -2, cs(p).as_ptr());
                    }
                }
                (api.setfield)(l, -2, cs(set).as_ptr());
            }
            (api.setfield)(l, -2, cs(task).as_ptr());
        }
    }
    Ok(1)
}

const METRES_PER_UNIT: f32 = 0.0254;

/// A `x,y,z` vector.
fn vec3(s: &str) -> Result<Vec3, String> {
    let f: Vec<f32> = s.split(',').map(|x| x.trim().parse::<f32>().map_err(|_| format!("{s}: want x,y,z"))).collect::<Result<_, _>>()?;
    match f.as_slice() {
        [x, y, z] if x.is_finite() && y.is_finite() && z.is_finite() => Ok(Vec3::new(*x, *y, *z)),
        _ => Err(format!("{s}: want x,y,z")),
    }
}

/// `<kind> [key=value ...]` as a reaction (points in the game's frame, m; `DMS.FollowReact`).
pub fn parse(line: &str) -> Result<React, String> {
    let mut w = line.split_whitespace();
    let kind = w.next().ok_or("an empty reaction")?;
    let mut kv = std::collections::HashMap::new();
    for x in w {
        let (k, v) = x.split_once('=').ok_or_else(|| format!("{x}: want key=value"))?;
        kv.insert(k, v);
    }
    let v = |k: &str| kv.get(k).map(|s| vec3(s)).transpose();
    let need = |k: &str| v(k)?.ok_or_else(|| format!("{kind} needs {k}=x,y,z"));
    let num = |k: &str, or: f32| kv.get(k).map_or(Ok(or), |s| s.parse::<f32>().map_err(|_| format!("{k}={s}: not a number")));
    let part = kv.get("part").map(|s| s.parse::<usize>().map_err(|_| format!("part={s}: not a part"))).transpose()?;
    if part.is_some_and(|p| p >= follow::PARTS) {
        return Err(format!("part: 0 to {}", follow::PARTS - 1));
    }
    Ok(match kind {
        "shot" => React::Shot {
            part,
            aim: match (v("at")?, v("dir")?) {
                (Some(a), Some(d)) => Some((a, d)),
                (None, None) => None,
                _ => return Err("shot: at= and dir= together".into()),
            },
            weapon: follow::weapon(kv.get("weapon").copied().unwrap_or("pistol")),
            fatal: num("fatal", 0.0)? != 0.0,
            more: kv.get("more").map_or(Ok(vec![]), |s| s.split(',').map(|k| k.parse::<usize>().map_err(|_| format!("more={s}: parts"))).collect())?,
        },
        "prop" => React::Prop { at: need("at")?, impulse: need("impulse")? },
        "blast" => React::Blast { at: need("at")? },
        "burn" => React::Burn,
        "taser" => React::Taser { part, at: v("at")? },
        "melee" => React::Melee { damage: num("damage", 10.0)?, at: need("at")?, impulse: v("impulse")?.unwrap_or(Vec3::ZERO), head: need("head")? },
        "nearmiss" => React::NearMiss { from: need("from")?, dir: need("dir")? },
        "car" => React::Car { id: num("id", 0.0)? as i32, impulse: need("impulse")?, at: need("at")?, player: num("player", 0.0)? != 0.0 },
        "thrown" => React::Thrown { from: need("from")?, vel: v("vel")?.unwrap_or(Vec3::ZERO) },
        "dropped" => React::Dropped,
        "poke" => React::Poke { part, at: need("at")?, dir: need("dir")? },
        "button" => React::Button(kv.get("name").map(|s| s.replace('_', " ")).ok_or("button needs name=<button> (STOP_all for `STOP all`)")?),
        "health" => React::Health(num("value", 200.0)?),
        "kill" => React::Shot { part, aim: None, weapon: 0, fatal: true, more: vec![] },
        _ => return Err(format!("{kind}: shot, prop, blast, burn, taser, melee, nearmiss, car, thrown, dropped, poke, button, health or kill")),
    })
}

native!(follow_react => follow_react_impl);
/// `DMS.FollowReact(id, line)`: the Playroom room's reaction to something the game did, for the agent's next NM step.
/// `line` is `<kind> [key=value ...]`, points `x,y,z` in the game's frame (m):
/// `shot [part=k] [at= dir=] [weapon=pistol|deagle|smg|ak47|sniper|shotgun|...] [fatal=1] [more=k,k]`,
/// `prop at= impulse=`, `blast at=`, `burn`,
/// `taser [part=k] [at=]`, `melee damage= at= impulse= head=`, `nearmiss from= dir=`, `car impulse= at= [id=]
/// [player=1]`, `thrown from= vel=`, `dropped`, `poke [part=k] at= dir=`, `button name=<Playroom button>`,
/// `health value=`, `kill`.
unsafe fn follow_react_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    let line = unsafe { api.check_string(l, 2)? };
    let r = parse(&line)?;
    let found = step::with(id, |f| f.react(r)).is_some();
    unsafe { (api.pushboolean)(l, found.into()) };
    Ok(1)
}

/// Source's angles (pitch, yaw, roll, degrees) of a rotation (`MatrixAngles`).
fn angles(q: Quat) -> [f32; 3] {
    let (f, left, up) = (q * Vec3::X, q * Vec3::Y, q * Vec3::Z);
    let xy = (f.x * f.x + f.y * f.y).sqrt();
    let (p, y, r) = if xy > 0.001 {
        ((-f.z).atan2(xy), f.y.atan2(f.x), left.z.atan2(up.z))
    } else {
        ((-f.z).atan2(xy), (-left.x).atan2(left.y), 0.0)
    };
    [p.to_degrees(), y.to_degrees(), r.to_degrees()]
}

native!(follow_pose => follow_pose_impl);
/// `DMS.FollowPose(id)`: while Homer gets up by animation, the clip's pose to hold his bodies on: a flat table, 7
/// numbers a part (its centre in units, its angles in degrees, then nothing: 6 used), nil otherwise.
unsafe fn follow_pose_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    let pose = step::with(id, |f| f.rise_pose().map(|p| (p, f.origin()))).flatten();
    unsafe {
        match pose {
            Some((p, o)) => {
                (api.createtable)(l, (p.len() * 6) as c_int, 0);
                let mut i = 0;
                for t in &p {
                    let x = (t.translation + o) / METRES_PER_UNIT;
                    for n in x.to_array().into_iter().chain(angles(t.rotation)) {
                        i += 1;
                        (api.pushnumber)(l, f64::from(n));
                        api.append(l, i);
                    }
                }
            }
            None => (api.pushnil)(l),
        }
    }
    Ok(1)
}

native!(follow_rise_done => follow_rise_done_impl);
/// `DMS.FollowRiseDone(id)`: Lua let the bodies go (after the `unfreeze` event): NM takes them on its next step.
unsafe fn follow_rise_done_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    unsafe { (api.pushboolean)(l, step::with(id, |f| f.rise_done()).is_some().into()) };
    Ok(1)
}

native!(follow_rise_cancel => follow_rise_cancel_impl);
/// `DMS.FollowRiseCancel(id)`: the get-up clip given up (Lua let the bodies go before its end, docs/adr/0021): a new
/// agent takes them as they are, balancing. False if he wasn't on a clip (or no such agent).
unsafe fn follow_rise_cancel_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    unsafe { (api.pushboolean)(l, step::with(id, |f| f.rise_cancel()).unwrap_or(false).into()) };
    Ok(1)
}

native!(follow_room => follow_room_impl);
/// `DMS.FollowRoom(id)`: the room's state of the agent, for the face and the HUD: `{health, dead, rising, shot, blast
/// (the ShotTask, the explosion task running), frozen, burning, shock, getup, ring_part, ring_age, ring_on (the hit on the part, from its centre in its frame, m),
/// face_centre, face_fwd, face_up (the Playroom's face on the head: in its part's frame, the centre from the part's
/// centre, m), face_radius (m)}`, nil for no such agent.
unsafe fn follow_room_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    let done = step::with(id, |f| unsafe {
        let r = &f.room;
        (api.createtable)(l, 0, 12);
        api.set_number(l, c"health", f64::from(r.health));
        api.set_bool(l, c"dead", r.dead);
        api.set_bool(l, c"rising", r.rise.is_some());
        let (shot, blast) = r.tasks();
        api.set_bool(l, c"shot", shot);
        api.set_bool(l, c"blast", blast);
        api.set_bool(l, c"frozen", f.frozen);
        api.set_number(l, c"burning", f64::from(r.burning.max(0.0)));
        api.set_number(l, c"shock", f64::from(r.shock.max(0.0)));
        api.set_str(l, c"getup", follow::GETUP_MODES[r.set.getup as usize % 4]);
        let (c, fwd, up, radius) = f.face();
        api.set_vector(l, c"face_centre", &c.to_array());
        api.set_vector(l, c"face_fwd", &fwd.to_array());
        api.set_vector(l, c"face_up", &up.to_array());
        api.set_number(l, c"face_radius", f64::from(radius));
        if let Some(g) = r.ring {
            api.set_number(l, c"ring_part", g.part as f64);
            api.set_number(l, c"ring_age", f64::from(g.age));
            api.set_vector(l, c"ring_on", &g.on.to_array());
        }
    });
    if done.is_none() {
        unsafe { (api.pushnil)(l) };
    }
    Ok(1)
}

native!(follow_overlay => follow_overlay_impl);
/// Numbers a part in `DMS.FollowOverlay`.
pub const OVERLAY_PART: usize = 25;

/// `DMS.FollowOverlay(id)`: what the debug overlay draws (NM's view after its last step, as the Playroom's `draw`
/// and `skeleton` have it), a flat table, 25 numbers a part (units unless said): its capsule's ends (3 + 3) and
/// radius, its box's half sizes (0 for a capsule), the capsule's rotation (x y z w), its joint's pivot (its centre
/// for the root), its parent (-1 for the root), its drive's force (N) and torque (N m), 1 if NM sees it touch; then
/// the centre of mass and the balancer's state (1 balancing, 2 fallen, 0 neither). Nil for no such agent.
unsafe fn follow_overlay_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    let n = step::with(id, |f| {
        let (d, o, build) = (&f.agent.doll, f.origin(), &f.def.build);
        let u = |p: Vec3| ((p + o) / METRES_PER_UNIT).to_array();
        let frames: Vec<Transform> = d.frames().collect();
        let mut n: Vec<f32> = Vec::with_capacity(follow::PARTS * OVERLAY_PART + 4);
        for (k, (c, p)) in d.capsules().zip(&build.parts).enumerate() {
            let a = c.rotation * Vec3::Y * p.half;
            n.extend(u(c.translation - a));
            n.extend(u(c.translation + a));
            n.push(p.radius / METRES_PER_UNIT);
            n.extend((p.boxed.unwrap_or(Vec3::ZERO) / METRES_PER_UNIT).to_array());
            n.extend(c.rotation.to_array());
            let fr = frames[k];
            n.extend(u(if p.parent.is_some() { fr.translation + fr.rotation * p.anchor } else { fr.translation }));
            n.push(p.parent.map_or(-1.0, |j| j as f32));
            let (fo, to) = f.drive(k);
            n.extend(fo.to_array());
            n.extend(to.to_array());
            n.push(f32::from(d.touch.get(k).is_some_and(|&t| t != 0)));
        }
        n.extend(u(d.com().0));
        n.push(f32::from(f.agent.balancing()) + 2.0 * f32::from(f.agent.fallen));
        n
    });
    unsafe {
        match n {
            Some(n) => {
                (api.createtable)(l, n.len() as c_int, 0);
                for (i, x) in n.iter().enumerate() {
                    (api.pushnumber)(l, f64::from(*x));
                    api.append(l, i + 1);
                }
            }
            None => (api.pushnil)(l),
        }
    }
    Ok(1)
}

/// `key=value` switches over `s` (the Playroom's `settings.txt` names).
fn apply(s: &mut Settings, line: &str) -> Result<(), String> {
    for kv in line.split_whitespace() {
        let (k, v) = kv.split_once('=').ok_or_else(|| format!("{kv}: want key=value"))?;
        follow::apply_switch(s, k, v)?;
    }
    Ok(())
}

native!(set_reactions => set_reactions_impl);
/// `DMS.SetReactions(line)`: the Playroom's switches for every Homer not running a scenario, `key=value ...` over its
/// defaults (`settings.txt`'s names: tasks, look, avoid, getup 0-3, extra_reactions, extra_pick, fire_reaction,
/// dive_shot, dodge_gap, weak, god).
unsafe fn set_reactions_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let line = unsafe { api.check_string(l, 1)? };
    let mut s = Settings::default();
    apply(&mut s, &line)?;
    follow::set_settings(s);
    Ok(0)
}

native!(reactions => reactions_impl);
/// `DMS.Reactions()`: what Lua can react with: `{buttons = {...}, extra = {...}, getup = {...}, weapons = {...}}`.
unsafe fn reactions_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    unsafe {
        (api.createtable)(l, 0, 4);
        let list = |name: &CStr, items: &[&str]| {
            (api.createtable)(l, items.len() as c_int, 0);
            for (i, s) in items.iter().enumerate() {
                api.push_str(l, s);
                api.append(l, i + 1);
            }
            (api.setfield)(l, -2, name.as_ptr());
        };
        list(c"buttons", &follow::BUTTONS);
        list(c"extra", &follow::EXTRA);
        list(c"getup", &follow::GETUP_MODES);
        list(c"weapons", &follow::WEAPONS.map(|w| w.0));
    }
    Ok(1)
}

native!(set_tuning => set_tuning_impl);
/// `DMS.SetTuning(name, lines)`: tuning on with the profile `name` (`msg <message> <param> <value>` and `tp <task>
/// <set> <param> <value>` lines, docs/adr/0009), from Homers' next steps; `DMS.SetTuning()` turns it off. The number
/// of values.
unsafe fn set_tuning_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let d = data::get().ok_or("NM's data isn't loaded (DMS.LoadData)")?;
    let name = unsafe { api.check_string(l, 1) }.ok();
    let n = match name {
        Some(name) => {
            let text = unsafe { api.check_string(l, 2)? };
            tuning::set(Some((&name, &text)), &d.tp)?
        }
        None => tuning::set(None, &d.tp)?,
    };
    unsafe { (api.pushnumber)(l, n as f64) };
    Ok(1)
}

native!(weapons => weapons_impl);
/// `DMS.Weapons()`: weaponinfo.xml's weapons the Playroom shoots with, `{name = {damage, force}}` (empty without the
/// file: its fallbacks).
unsafe fn weapons_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let d = data::get();
    unsafe {
        (api.createtable)(l, 0, follow::WEAPONS.len() as c_int);
        for (name, game, _) in follow::WEAPONS {
            let Some(w) = d.as_ref().and_then(|d| d.weapons.iter().find(|w| w.name == game)) else { continue };
            (api.createtable)(l, 0, 2);
            api.set_number(l, c"damage", f64::from(w.damage));
            api.set_number(l, c"force", f64::from(w.force));
            let n = std::ffi::CString::new(name).unwrap_or_default();
            (api.setfield)(l, -2, n.as_ptr());
        }
    }
    Ok(1)
}
