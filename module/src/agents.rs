//! The FOLLOW agents' natives (ROADMAP M4): Lua binds Homer's ragdoll (`DMS.BindHomer`), puts NM on it
//! (`DMS.Follow`), sends it messages, takes it off before the ragdoll goes, and hears of it through `DMS.FollowEvents`
//! in `Think` (docs/adr/0003).

use std::ffi::{CStr, c_int};
use std::sync::Arc;

use delirium::prelude::*;
use delirium::ragdoll::Obstacle;

use crate::data;
use crate::ffi::vphysics_nm::{self, IPhysicsConstraint, IPhysicsObject, Version};
use crate::follow::{Follower, GravityMode, PARTS};
use crate::lua::{self, Api, LuaState, TTABLE, native};
use crate::step;
use crate::world;

pub const NATIVES: &[(&CStr, lua::CFunction)] = &[
    (c"Follow", follow),
    (c"FollowObstacles", follow_obstacles),
    (c"Unfollow", unfollow),
    (c"FollowSend", follow_send),
    (c"FollowList", follow_list),
    (c"FollowState", follow_state),
    (c"FollowEvents", follow_events),
    (c"FollowGravityOff", follow_gravity_off),
    (c"SetGravity", set_gravity),
    (c"FollowCost", follow_cost),
    (c"EnvTimestep", env_timestep),
];

native!(env_timestep => env_timestep_impl);
/// `DMS.EnvTimestep(physobj[, t])`: the physics step (s) of the object's environment (the game sets 0.015 whatever
/// the tickrate); with `t`, sets it first. For measuring NM's 1/60 against it (ROADMAP M4).
unsafe fn env_timestep_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    let o = unsafe { api.check_physobj(l, 1)? };
    // SAFETY: a PhysObj Lua handed over.
    let env = unsafe { nm.object_environment(o) };
    if env.is_null() {
        return Err("no environment".into());
    }
    let t = unsafe { api.opt_number(l, 2, -1.0)? } as f32;
    // SAFETY: the object's live environment, from a native (outside Simulate).
    unsafe {
        if t > 0.0 {
            if !(0.001..=0.1).contains(&t) {
                return Err("a step from 0.001 to 0.1 s".into());
            }
            vphysics_nm::env::set_timestep(env, t);
        }
        (api.pushnumber)(l, f64::from(vphysics_nm::env::timestep(env)));
    }
    Ok(1)
}

/// The 21 PhysObj of the table at `arg`, `[i + 1]` part `i`'s.
pub unsafe fn parts(api: &Api, l: *mut LuaState, arg: c_int) -> Result<Vec<*mut IPhysicsObject>, String> {
    if unsafe { (api.type_)(l, arg) } != TTABLE {
        return Err(format!("bad argument #{arg} (a table of {PARTS} PhysObj in NM's part order expected)"));
    }
    (0..PARTS)
        .map(|i| unsafe {
            (api.rawgeti)(l, arg, i as c_int + 1);
            let p = api.check_physobj(l, -1).map_err(|_| format!("bad argument #{arg} ([{}] isn't a PhysObj)", i + 1));
            (api.settop)(l, -2);
            p
        })
        .collect()
}

/// NM on the ragdoll `objects` (bound by `DMS.BindHomer`): its joints found, the agent made on them as they are now.
/// NM's frame is at `floor`; its world the map's (`world.rs`), or without one the Playroom's floor there (with the
/// room's pool for `room`, the bench).
///
/// # Safety
/// The objects are live until the agent is unfollowed.
pub unsafe fn start(objects: Vec<*mut IPhysicsObject>, yaw: f32, floor: Vec3, room: bool) -> Result<u32, String> {
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available (vphysics.dll isn't VPhysics-Box3D)")?;
    if nm.version != Version::V003 {
        return Err(format!("{} is too old for Homer: update VPhysics-Box3D", nm.version.name()));
    }
    let d = data::get().ok_or_else(|| format!("NM's data isn't loaded (DMS.LoadData): {}", data::how_to()))?;
    let joints = d
        .body
        .parts
        .iter()
        .enumerate()
        .map(|(k, p)| match p.parent {
            None => Ok(std::ptr::null_mut::<IPhysicsConstraint>()),
            Some(q) => {
                // SAFETY: both are PhysObj of the ragdoll, alive for this call.
                let c = unsafe { nm.find_constraint(objects[q], objects[k]) };
                if c.is_null() { Err(format!("{}: no constraint to {} (not models/dms/homer.mdl?)", p.name, d.body.parts[q].name)) } else { Ok(c) }
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    // NM's frame starts on the floor under him, as the Playroom's room does under its Homer: its world is the map's
    // (M5), its floor's squares round him; without a map, the Playroom's floor (its room's on the bench, with the pool).
    let origin = floor;
    let world = world::collision(origin).unwrap_or_else(|| Arc::new(data::floor_collision(&d, [0.0; 3], room)));
    // SAFETY: the caller's promise.
    unsafe { step::follow(|nm, id| Follower::new(nm, id, d.clone(), world, objects, joints, origin, yaw)) }
}

native!(follow => follow_impl);
/// `DMS.Follow(parts, yaw, floor[, room])`: NM drives Homer's ragdoll from the next physics step. `parts` its 21
/// PhysObj in NM's part order, bound (`DMS.BindHomer`); `yaw` where he faces (rad); `floor` the floor under him
/// (m, the game's frame: Box3D's, DMS_FloorBelow), where NM's frame starts; `room` true for the bench (the Playroom
/// room's floor if the map has no world). The agent's id.
/// `DMS.Unfollow` it before the ragdoll goes.
unsafe fn follow_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let objects = unsafe { parts(api, l, 1)? };
    let yaw = unsafe { api.check_number(l, 2)? } as f32;
    let floor = Vec3::from_array(unsafe { api.check_vec3(l, 3)? });
    let room = unsafe { (api.toboolean)(l, 4) } != 0;
    let id = unsafe { start(objects, yaw, floor, room)? };
    unsafe { (api.pushnumber)(l, f64::from(id)) };
    Ok(1)
}

native!(unfollow => unfollow_impl);
/// `DMS.Unfollow(id)`: NM off the ragdoll (armature 0, no drives): an ordinary ragdoll again. False for no such agent.
unsafe fn unfollow_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    unsafe { (api.pushboolean)(l, step::unfollow(id).into()) };
    Ok(1)
}

unsafe fn agent_id(api: &Api, l: *mut LuaState) -> Result<u32, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    step::with(id, |_| ()).ok_or_else(|| format!("no DMS agent {id}"))?;
    Ok(id)
}

native!(follow_send => follow_send_impl);
/// `DMS.FollowSend(id, line)`: a message line (`<message> [param=value ...]`, as the Playroom's console takes it) for
/// the agent's next NM step. Vectors are in NM's frame (from the floor he was put on; the bench's: the Playroom room's).
unsafe fn follow_send_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { agent_id(api, l)? };
    let line = unsafe { api.check_string(l, 2)? };
    step::with(id, |f| f.queue(line.trim())).unwrap_or(Ok(()))?;
    Ok(0)
}

native!(follow_list => follow_list_impl);
/// `DMS.FollowList()`: the agents' ids.
unsafe fn follow_list_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let ids = step::ids();
    unsafe {
        (api.createtable)(l, ids.len() as c_int, 0);
        for (i, id) in ids.iter().enumerate() {
            (api.pushnumber)(l, f64::from(*id));
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

native!(follow_state => follow_state_impl);
/// `DMS.FollowState(id)`: `{steps, awake, asleep, balancing, fallen, compensation, pelvis, head, com, com_v}` (m,
/// the game's frame; NM's view after its last step), nil for no such agent.
unsafe fn follow_state_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    let done = step::with(id, |f| unsafe {
        let d = &f.agent.doll;
        let o = f.origin();
        let frames: Vec<Transform> = d.frames().collect();
        let (com, v) = d.com();
        (api.createtable)(l, 0, 10);
        api.set_number(l, c"steps", f.steps as f64);
        api.set_bool(l, c"awake", d.awake());
        api.set_bool(l, c"asleep", f.asleep());
        api.set_bool(l, c"balancing", f.agent.balancing());
        api.set_bool(l, c"fallen", f.agent.fallen);
        api.set_number(l, c"compensation", f64::from(f.comp));
        api.set_vector(l, c"pelvis", &(frames[0].translation + o).to_array());
        api.set_vector(l, c"head", &(frames[12].translation + o).to_array());
        api.set_vector(l, c"com", &(com + o).to_array());
        api.set_vector(l, c"com_v", &v.to_array());
    });
    if done.is_none() {
        unsafe { (api.pushnil)(l) };
    }
    Ok(1)
}

native!(follow_events => follow_events_impl);
/// `DMS.FollowEvents()`: what happened since the last call, `{id, kind, text, value}` each: `feedback` (NM's,
/// `text` "name slot"), `fallen`, `up`, `sleep` (NM's doll sleeps: put the bodies to sleep), `wake`, `detached` (NaN:
/// NM is off the ragdoll, `text` why).
unsafe fn follow_events_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let events = step::events();
    unsafe {
        (api.createtable)(l, events.len() as c_int, 0);
        for (i, e) in events.iter().enumerate() {
            (api.createtable)(l, 0, 4);
            api.set_number(l, c"id", f64::from(e.id));
            api.set_str(l, c"kind", e.kind);
            api.set_str(l, c"text", &e.text);
            api.set_number(l, c"value", f64::from(e.value));
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

/// Numbers per obstacle in `DMS.FollowObstacles`' list.
const OBSTACLE_NUMBERS: usize = 25;

native!(follow_obstacles => follow_obstacles_impl);
/// `DMS.FollowObstacles(id, list)`: the props near the agent as NM's obstacles (boxes, as the Playroom's cars and
/// planks) for its steps until the next call. `list` is flat, 25 numbers a box (m, s, the game's frame): its id, the
/// box's centre, its forward, left and up axes, its half sizes, velocity, spin (rad/s), centre of mass. False for no
/// such agent.
unsafe fn follow_obstacles_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    if unsafe { (api.type_)(l, 2) } != TTABLE {
        return Err("bad argument #2 (a flat table of numbers expected)".into());
    }
    let mut n = vec![];
    for i in 1.. {
        unsafe { (api.rawgeti)(l, 2, i) };
        let v = unsafe { ((api.type_)(l, -1) == lua::TNUMBER).then(|| (api.tonumber)(l, -1) as f32) };
        unsafe { (api.settop)(l, -2) };
        match v {
            Some(x) if x.is_finite() => n.push(x),
            Some(_) => return Err(format!("bad argument #2 (number {i} isn't finite)")),
            None => break,
        }
    }
    if n.len() % OBSTACLE_NUMBERS != 0 {
        return Err(format!("bad argument #2 ({OBSTACLE_NUMBERS} numbers a box)"));
    }
    let found = step::with(id, |f| {
        let o = f.origin();
        f.obstacles = n
            .chunks(OBSTACLE_NUMBERS)
            .map(|b| {
                let v = |k: usize| Vec3::new(b[k], b[k + 1], b[k + 2]);
                let rot = Quat::from_mat3(&Mat3::from_cols(v(4).normalize_or_zero(), v(7).normalize_or_zero(), v(10).normalize_or_zero())).normalize();
                let centre = v(1) - o;
                // Box3D's shell around the prop's hull (docs/adr/0018).
                let half = v(13) + Vec3::splat(world::SHAPE_INFLATION);
                Obstacle { id: b[0] as i32, at: centre, rot, centre, half, vel: v(16), spin: v(19), com: v(22) - o, ..Default::default() }
            })
            .collect();
    });
    unsafe { (api.pushboolean)(l, found.is_some().into()) };
    Ok(1)
}

native!(follow_gravity_off => follow_gravity_off_impl);
/// `DMS.FollowGravityOff(id, mask)`: the parts the game doesn't pull (`PhysObj:IsGravityEnabled()` false), bit `k`
/// part `k`: their compensation takes Source's gravity as 0 (docs/adr/0007). False for no such agent (NM came off).
unsafe fn follow_gravity_off_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let id = unsafe { api.check_number(l, 1)? } as u32;
    let mask = unsafe { api.check_number(l, 2)? } as u32;
    let found = step::with(id, |f| f.gravity_off = mask).is_some();
    unsafe { (api.pushboolean)(l, found.into()) };
    Ok(1)
}

native!(set_gravity => set_gravity_impl);
/// `DMS.SetGravity(g, mode)`: Source's gravity (m/s^2, a vector, `physenv.GetGravity()` scaled) and how it's made
/// up to NM's 9.8 (`nm`, `hybrid`, `off`; docs/adr/0007).
unsafe fn set_gravity_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let g = Vec3::from_array(unsafe { api.check_vec3(l, 1)? });
    let m = unsafe { api.check_string(l, 2)? };
    let mode = GravityMode::parse(&m).ok_or_else(|| format!("{m}: nm, hybrid or off"))?;
    step::set_gravity(g, mode);
    Ok(0)
}

native!(follow_cost => follow_cost_impl);
/// `DMS.FollowCost()`: what the agents cost a physics step (ms, smoothed).
unsafe fn follow_cost_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    unsafe { (api.pushnumber)(l, step::cost_ms()) };
    Ok(1)
}
