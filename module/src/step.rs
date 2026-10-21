//! The step callback on the server's physics environment and the drives it applies every step.
//!
//! No Lua runs in here (docs/adr/0003): natives change the drive list between steps, the callback only reads it
//! and calls `IPhysicsNM`.

use std::ffi::c_void;
use std::ptr;
use std::sync::{Mutex, MutexGuard};

use crate::ffi::vphysics_nm::{self, IPhysicsEnvironment, IPhysicsObject};

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
}

// SAFETY: the pointers are only used on the game's main thread, where the physics and Lua both run.
unsafe impl Send for State {}

static STATE: Mutex<State> = Mutex::new(State { env: ptr::null_mut(), drives: Vec::new() });

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Drives `object` with `force` (N) and `torque` (Nm), world, every step: for `steps` steps or until [`remove`]d.
/// Replaces an earlier drive of the same object. Sets the callback on the object's environment.
///
/// # Safety
/// `object` is a live `IPhysicsObject` and stays so until it's removed (or its steps run out).
pub unsafe fn add(object: *mut IPhysicsObject, force: [f32; 3], torque: [f32; 3], steps: Option<u32>) -> Result<(), String> {
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available")?;
    let env = unsafe { nm.object_environment(object) };
    if env.is_null() {
        return Err("the object has no physics environment".into());
    }
    let mut state = state();
    if state.env != env {
        if !state.env.is_null() {
            // SAFETY: the old environment is still alive: callbacks are removed on ShutDown, before it goes.
            unsafe { nm.set_step_callback(state.env, None, ptr::null_mut()) };
            state.drives.clear();
        }
        // SAFETY: a live environment; the callback has no context.
        if !unsafe { nm.set_step_callback(env, Some(on_step), ptr::null_mut()) } {
            return Err("SetStepCallback failed".into());
        }
        state.env = env;
    }
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
    if state.drives.is_empty() {
        unset(&mut state);
    }
    removed
}

/// How many objects are driven.
pub fn count() -> usize {
    state().drives.len()
}

/// Drops every drive and removes the callback. For `ShutDown` and the module's unloading.
pub fn shutdown() {
    let mut state = state();
    state.drives.clear();
    unset(&mut state);
}

fn unset(state: &mut State) {
    if state.env.is_null() {
        return;
    }
    if let Some(nm) = vphysics_nm::get() {
        // SAFETY: the environment the callback was set on, still alive (see `add`).
        unsafe { nm.set_step_callback(state.env, None, ptr::null_mut()) };
    }
    state.env = ptr::null_mut();
}

/// `PhysicsNMStepFn_t`: applies every drive for the step about to be solved.
unsafe extern "C" fn on_step(env: *mut IPhysicsEnvironment, _timestep: f32, _context: *mut c_void) {
    let Some(nm) = vphysics_nm::get() else { return };
    let mut state = state();
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
}
