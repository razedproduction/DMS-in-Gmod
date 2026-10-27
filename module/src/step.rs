//! The step callback on the server's physics environment: the FOLLOW agents (`follow.rs`, ROADMAP M4) and the
//! debug drives of `DMS.Drive`.
//!
//! No Lua runs in here (docs/adr/0003): natives change the lists between steps, the callback only steps the agents
//! and calls `IPhysicsNM`; what Lua should hear of comes out through the event queue, taken in `Think`.
//!
//! Each Source step (docs/adr/0021): every agent reads its bodies from Box3D on this thread (`Follower::begin`), the
//! agents' NM steps run on up to `threads` threads at once (`Follower::compute`: pure, each agent on its own), and
//! the armature and drives go back on this thread (`Follower::finish`). A panic takes only its agent off.

use std::any::Any;
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use delirium::prelude::Vec3;

use crate::ffi::vphysics_nm::{self, IPhysicsEnvironment, IPhysicsObject, Nm};
use crate::follow::{Event, Finished, Follower, GravityMode};

/// Events kept for Lua at most; older ones go.
const MAX_EVENTS: usize = 4096;
/// Threads the agents step on at most, and a thread's stack (NM's step is deep).
const MAX_THREADS: usize = 16;
const STACK: usize = 8 << 20;

/// `DMS.DevPanic("step")`: the next agent's step panics, to show the callback survives it.
pub static PANIC_NEXT: AtomicBool = AtomicBool::new(false);

/// A panic's message.
pub fn panic_text(p: &(dyn Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "?".into())
}

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
    /// Threads the agents step on (`DMS.SetThreads`, 0: the CPU's cores less one).
    threads: usize,
    /// Callback calls: all, and those from an environment it isn't set on (a listen server's client's, docs/adr/0021).
    calls: u64,
    foreign: u64,
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
    threads: 0,
    calls: 0,
    foreign: 0,
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
    attach(&nm, &mut state, f.anchor())?;
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

/// LOD (docs/adr/0021): agent `id` parked (NM off its bodies, the agent kept; `on_motion`: it wakes once they move)
/// or not. False for no such agent.
pub fn park(id: u32, on: bool, on_motion: bool) -> bool {
    let mut state = state();
    let Some(f) = state.followers.iter_mut().find(|f| f.id == id) else { return false };
    match (on, vphysics_nm::get()) {
        // SAFETY: its ragdoll is alive until Lua unfollows it.
        (true, Some(nm)) => unsafe { f.park(&nm, on_motion) },
        (true, None) => {}
        (false, _) => f.unpark(),
    }
    true
}

/// Threads the agents step on (0: the CPU's cores less one).
pub fn set_threads(n: usize) {
    state().threads = n.min(MAX_THREADS);
}

/// `(threads used, callback calls, calls from another environment)`.
pub fn stats() -> (usize, u64, u64) {
    let s = state();
    (threads(s.threads), s.calls, s.foreign)
}

fn threads(setting: usize) -> usize {
    match setting {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1)).clamp(1, MAX_THREADS),
        n => n,
    }
}

/// Drops every drive and agent and removes the callback. For `ShutDown` and the module's unloading.
pub fn shutdown() {
    let mut state = state();
    unset(&mut state);
    // Their buffers too: the module may be unloaded next.
    state.drives = Vec::new();
    state.followers = Vec::new();
    state.events = Vec::new();
    state.finished = Vec::new();
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

/// `PhysicsNMStepFn_t`: steps the agents and applies every drive for the step about to be solved. A panic stops
/// at it (docs/adr/0021): whatever it was in the middle of, every agent comes off and the game goes on.
unsafe extern "C" fn on_step(env: *mut IPhysicsEnvironment, timestep: f32, _context: *mut c_void) {
    // SAFETY: the callback's own arguments.
    let Err(p) = panic::catch_unwind(AssertUnwindSafe(|| unsafe { step(env, timestep) })) else { return };
    let text = format!("panic: {}", panic_text(&*p));
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut state = state();
        state.drives.clear();
        let nm = vphysics_nm::get();
        for f in std::mem::take(&mut state.followers) {
            if let Some(nm) = &nm {
                // SAFETY: its ragdoll is alive (Lua hasn't unfollowed it).
                unsafe { f.release(nm) };
            }
            state.events.push(Event { id: f.id, kind: "detached", text: text.clone(), value: 0.0 });
        }
    }));
}

/// An agent's NM steps, a panic in them its error.
fn compute(f: &mut Follower) -> (Result<(), String>, Vec<Event>) {
    let mut events = vec![];
    let r = panic::catch_unwind(AssertUnwindSafe(|| {
        if f.due > 0 && PANIC_NEXT.swap(false, Ordering::Relaxed) {
            panic!("DMS.DevPanic");
        }
        f.compute(&mut events)
    }));
    (r.unwrap_or_else(|p| Err(format!("panic: {}", panic_text(&*p)))), events)
}

/// [`compute`] for every agent whose `begin` went well, on up to `threads` threads (this one too); their results in
/// the agents' order.
fn compute_all(followers: &mut [Follower], ok: &[bool], threads: usize) -> Vec<(Result<(), String>, Vec<Event>)> {
    let busy = followers.iter().zip(ok).filter(|(f, ok)| **ok && f.due > 0).count();
    let run = |f: &mut Follower, ok: bool| if ok { compute(f) } else { (Ok(()), vec![]) };
    if busy < 2 || threads < 2 {
        return followers.iter_mut().zip(ok).map(|(f, &ok)| run(f, ok)).collect();
    }
    let chunk = followers.len().div_ceil(threads.min(busy));
    let mut out: Vec<(Result<(), String>, Vec<Event>)> = Vec::with_capacity(followers.len());
    std::thread::scope(|s| {
        let mut parts = followers.chunks_mut(chunk).zip(ok.chunks(chunk));
        let mine = parts.next();
        let handles: Vec<_> = parts
            .map(|(fs, oks)| {
                let n = fs.len();
                let job = move || fs.iter_mut().zip(oks).map(|(f, &ok)| run(f, ok)).collect::<Vec<_>>();
                (n, std::thread::Builder::new().stack_size(STACK).spawn_scoped(s, job).map_err(|e| e.to_string()))
            })
            .collect();
        if let Some((fs, oks)) = mine {
            out.extend(fs.iter_mut().zip(oks).map(|(f, &ok)| run(f, ok)));
        }
        for (n, h) in handles {
            // `compute` catches its panics; no thread for them, those agents come off (their results stay in order).
            match h.and_then(|h| h.join().map_err(|p| panic_text(&*p))) {
                Ok(r) => out.extend(r),
                Err(e) => out.extend(std::iter::repeat_with(|| (Err(format!("no thread: {e}")), vec![])).take(n)),
            }
        }
    });
    out
}

unsafe fn step(env: *mut IPhysicsEnvironment, timestep: f32) {
    let Some(nm) = vphysics_nm::get() else { return };
    let mut guard = state();
    let state = &mut *guard;
    state.calls += 1;
    if state.env != env {
        state.foreign += 1;
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
    let mut ok = Vec::with_capacity(state.followers.len());
    for f in &mut state.followers {
        // SAFETY: agents are unfollowed before their ragdolls go.
        let r = unsafe { f.begin(&nm, timestep, &mut state.events) };
        ok.push(r.is_ok());
        if let Err(e) = r {
            failed.push((f.id, e));
        }
    }
    let results = compute_all(&mut state.followers, &ok, threads(state.threads));
    for (f, (r, events)) in state.followers.iter_mut().zip(results) {
        state.events.extend(events);
        match r {
            // SAFETY: as above.
            Ok(()) if !failed.iter().any(|(id, _)| *id == f.id) => unsafe { f.finish(&nm, g, mode) },
            Ok(()) => {}
            Err(e) => failed.push((f.id, e)),
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
