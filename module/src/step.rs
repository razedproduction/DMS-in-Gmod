//! The step callback on the server's physics environment: the FOLLOW agents (`follow.rs`, ROADMAP M4) and the
//! debug drives of `DMS.Drive`.
//!
//! No Lua runs in here (docs/adr/0003): natives change the lists between steps, the callback only steps the agents
//! and calls `IPhysicsNM`; what Lua should hear of comes out through the event queue, taken in `Think`.

use std::ffi::c_void;
use std::ptr;
use std::sync::{Mutex, MutexGuard};

use delirium::prelude::Vec3;

use crate::ffi::vphysics_nm::{self, IPhysicsEnvironment, IPhysicsObject, Nm};
use crate::follow::{Event, Finished, Follower, GravityMode};

/// Events kept for Lua at most; older ones go.
const MAX_EVENTS: usize = 4096;

/// A force and torque applied to one object every step.
struct Drive {
    object: *mut IPhysicsObject,
    force: [f32; 3],
    torque: [f32; 3],
    /// Steps left, `None` until removed.
    steps: Option<u32>,
}

struct State {
    /// The environment the callback is set on, null for none.
    env: *mut IPhysicsEnvironment,
    drives: Vec<Drive>,
    followers: Vec<Follower>,
    next_id: u32,
    /// Source's gravity (m/s^2, from `physenv.GetGravity()` through `DMS.SetGravity`) and how it's made up.
    g_src: Vec3,
    mode: GravityMode,
    events: Vec<Event>,
    /// Scenarios that ended, by agent.
    finished: Vec<(u32, Finished)>,
    /// NM steps' time over the last Source steps (s), for `dms_status`.
    cost: f64,
}

// SAFETY: the pointers are only used on the game's main thread, where the physics and Lua both run.
unsafe impl Send for State {}

static STATE: Mutex<State> = Mutex::new(State {
    env: ptr::null_mut(),
    drives: Vec::new(),
    followers: Vec::new(),
    next_id: 1,
    g_src: Vec3::new(0.0, 0.0, -15.24),
    mode: GravityMode::Nm,
    events: Vec::new(),
    finished: Vec::new(),
    cost: 0.0,
});

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sets the callback on `object`'s environment (moving it from another one, whose drives and agents go).
fn attach(nm: &Nm, state: &mut State, object: *mut IPhysicsObject) -> Result<(), String> {
    // SAFETY: a live object (the callers').
    let env = unsafe { nm.object_environment(object) };
    if env.is_null() {
        return Err("the object has no physics environment".into());
    }
    if state.env == env {
        return Ok(());
    }
    if !state.env.is_null() {
        // SAFETY: the old environment is still alive: callbacks are removed on ShutDown, before it goes.
        unsafe { nm.set_step_callback(state.env, None, ptr::null_mut()) };
        state.drives.clear();
        for f in state.followers.drain(..) {
            // SAFETY: its ragdoll is alive until Lua unfollows it, which it hasn't.
            unsafe { f.release(nm) };
        }
    }
    // SAFETY: a live environment; the callback has no context.
    if !unsafe { nm.set_step_callback(env, Some(on_step), ptr::null_mut()) } {
        return Err("SetStepCallback failed".into());
    }
    state.env = env;
    Ok(())
}

/// Drives `object` with `force` (N) and `torque` (Nm), world, every step: for `steps` steps or until [`remove`]d.
/// Replaces an earlier drive of the same object. Sets the callback on the object's environment.
///
/// # Safety
/// `object` is a live `IPhysicsObject` and stays so until it's removed (or its steps run out).
pub unsafe fn add(object: *mut IPhysicsObject, force: [f32; 3], torque: [f32; 3], steps: Option<u32>) -> Result<(), String> {
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    let mut state = state();
    attach(&nm, &mut state, object)?;
    state.drives.retain(|d| d.object != object);
    state.drives.push(Drive { object, force, torque, steps });
    Ok(())
}

/// Stops driving `object`; true if it was driven. The callback goes once nothing is left.
pub fn remove(object: *mut IPhysicsObject) -> bool {
    let mut state = state();
    let before = state.drives.len();
    state.drives.retain(|d| d.object != object);
    let removed = state.drives.len() != before;
    unset_if_idle(&mut state);
    removed
}

/// How many objects are driven.
pub fn count() -> usize {
    state().drives.len()
}

/// Starts stepping a FOLLOW agent made by `make` with its id; the id.
///
/// # Safety
/// The agent's objects and joints are live until [`unfollow`].
pub unsafe fn follow(make: impl FnOnce(&Nm, u32) -> Result<Follower, String>) -> Result<u32, String> {
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    let mut state = state();
    let id = state.next_id;
    let f = make(&nm, id)?;
    attach(&nm, &mut state, f.objects()[0])?;
    state.next_id += 1;
    state.followers.push(f);
    Ok(id)
}

/// Takes agent `id` off its ragdoll (no armature, no drives); false if there's none. Lua calls it before the
/// ragdoll's objects go.
pub fn unfollow(id: u32) -> bool {
    let mut state = state();
    let Some(i) = state.followers.iter().position(|f| f.id == id) else { return false };
    let f = state.followers.remove(i);
    if let Some(nm) = vphysics_nm::get() {
        // SAFETY: Lua unfollows before the ragdoll goes.
        unsafe { f.release(&nm) };
    }
    state.finished.retain(|(i, _)| *i != id);
    unset_if_idle(&mut state);
    true
}

/// Runs `f` on agent `id`.
pub fn with<R>(id: u32, f: impl FnOnce(&mut Follower) -> R) -> Option<R> {
    state().followers.iter_mut().find(|a| a.id == id).map(f)
}

/// The agents' ids.
pub fn ids() -> Vec<u32> {
    state().followers.iter().map(|f| f.id).collect()
}

/// Every agent's telemetry (JSON objects).
pub fn telemetry() -> Vec<String> {
    state().followers.iter().map(Follower::telemetry).collect()
}

/// Sets Source's gravity (m/s^2) and the compensation's mode.
pub fn set_gravity(g: Vec3, mode: GravityMode) {
    let mut state = state();
    if g.is_finite() {
        state.g_src = g;
    }
    state.mode = mode;
}

/// Source's gravity and the compensation's mode.
pub fn gravity() -> (Vec3, GravityMode) {
    let s = state();
    (s.g_src, s.mode)
}

/// The events since the last call.
pub fn events() -> Vec<Event> {
    std::mem::take(&mut state().events)
}

/// The scenarios that ended since the last call, by agent.
pub fn finished() -> Vec<(u32, Finished)> {
    std::mem::take(&mut state().finished)
}

/// The NM steps' cost over the last second of Source steps (ms per Source step, smoothed).
pub fn cost_ms() -> f64 {
    state().cost * 1000.0
}

/// Drops every drive and agent and removes the callback. For `ShutDown` and the module's unloading.
pub fn shutdown() {
    let mut state = state();
    state.drives.clear();
    state.followers.clear();
    state.events.clear();
    state.finished.clear();
    unset(&mut state);
}

fn unset_if_idle(state: &mut State) {
    if state.drives.is_empty() && state.followers.is_empty() {
        unset(state);
    }
}

fn unset(state: &mut State) {
    if state.env.is_null() {
        return;
    }
    if let Some(nm) = vphysics_nm::get() {
        // SAFETY: the environment the callback was set on, still alive (see `attach`).
        unsafe { nm.set_step_callback(state.env, None, ptr::null_mut()) };
    }
    state.env = ptr::null_mut();
}

/// `PhysicsNMStepFn_t`: steps the agents and applies every drive for the step about to be solved.
unsafe extern "C" fn on_step(env: *mut IPhysicsEnvironment, timestep: f32, _context: *mut c_void) {
    let Some(nm) = vphysics_nm::get() else { return };
    let mut guard = state();
    let state = &mut *guard;
    if state.env != env {
        return;
    }
    state.drives.retain_mut(|d| {
        // SAFETY: drives are removed before their objects go (see `add`).
        unsafe { nm.apply_body_drive(d.object, Some(&d.force), Some(&d.torque)) };
        match &mut d.steps {
            Some(n) => {
                *n -= 1;
                *n > 0
            }
            None => true,
        }
    });
    let started = std::time::Instant::now();
    let (g, mode) = (state.g_src, state.mode);
    let mut failed = vec![];
    for f in &mut state.followers {
        // SAFETY: agents are unfollowed before their ragdolls go.
        if let Err(e) = unsafe { f.tick(&nm, timestep, g, mode, &mut state.events) } {
            failed.push((f.id, e));
        }
        if f.script.as_ref().is_some_and(|s| s.done) {
            let s = f.script.take().expect("a done script");
            state.finished.push((f.id, s.finish()));
        }
    }
    // docs/adr/0003, ROADMAP M4: a NaN in the state or the drives takes NM off; the ragdoll stays an ordinary one.
    for (id, e) in failed {
        if let Some(i) = state.followers.iter().position(|f| f.id == id) {
            let f = state.followers.remove(i);
            // SAFETY: its ragdoll is alive (Lua hasn't unfollowed it).
            unsafe { f.release(&nm) };
        }
        state.events.push(Event { id, kind: "detached", text: e, value: 0.0 });
    }
    let n = state.events.len();
    if n > MAX_EVENTS {
        state.events.drain(..n - MAX_EVENTS);
    }
    let spent = started.elapsed().as_secs_f64();
    state.cost += (spent - state.cost) * 0.02;
}
