//! Debug natives on the `DMS` table: direct access to `IPhysicsNM` from Lua (ROADMAP M1).
//!
//! Vectors are SI and world, as `IPhysicsNM` takes them. Vector arguments are a `Vector` or `{x, y, z}`.

use std::ffi::{CStr, c_int};

use crate::ffi::vphysics_nm::{self, MAX_ARMATURE, Nm};
use crate::lua::{self, Api, LuaState, TBOOLEAN, native};
use crate::step;

/// Every native, by its name in `DMS`.
pub const NATIVES: &[(&CStr, lua::CFunction)] = &[
    (c"Available", available),
    (c"InterfaceVersion", interface_version),
    (c"BodyState", body_state),
    (c"BodyInertia", body_inertia),
    (c"SetBodyState", set_body_state),
    (c"Drive", drive),
    (c"FindJoint", find_joint),
    (c"SetArmature", set_armature),
    (c"GetArmature", get_armature),
    (c"SubstepCount", substep_count),
    (c"Hover", hover),
    (c"Shutdown", shutdown),
];

fn nm() -> Result<Nm, String> {
    vphysics_nm::get().ok_or_else(|| "IPhysicsNM isn't available (vphysics.dll isn't VPhysics-Box3D)".into())
}

native!(available => available_impl);
/// `DMS.Available()`: whether the loaded vphysics has `IPhysicsNM`.
unsafe fn available_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    unsafe { (api.pushboolean)(l, vphysics_nm::get().is_some().into()) };
    Ok(1)
}

native!(interface_version => interface_version_impl);
/// `DMS.InterfaceVersion()`: `"VBOX3D_NM_003"`, `"VBOX3D_NM_002"`, `"VBOX3D_NM_001"` or nil.
unsafe fn interface_version_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    match vphysics_nm::get() {
        Some(nm) => unsafe { api.push_str(l, nm.version.name()) },
        None => unsafe { (api.pushnil)(l) },
    }
    Ok(1)
}

native!(body_state => body_state_impl);
/// `DMS.BodyState(physobj)`: `{pos, rot, vel, angvel}` (m, quaternion xyzw, m/s, rad/s), nil if it fails.
unsafe fn body_state_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = nm()?;
    let object = unsafe { api.check_physobj(l, 1)? };
    let Some(s) = (unsafe { nm.body_state(object) }) else {
        unsafe { (api.pushnil)(l) };
        return Ok(1);
    };
    unsafe {
        (api.createtable)(l, 0, 4);
        for (name, v) in [
            (c"pos", &s.position[..]),
            (c"rot", &s.rotation[..]),
            (c"vel", &s.velocity[..]),
            (c"angvel", &s.angular_velocity[..]),
        ] {
            api.push_vector(l, v);
            (api.setfield)(l, -2, name.as_ptr());
        }
    }
    Ok(1)
}

native!(set_body_state => set_body_state_impl);
/// `DMS.SetBodyState(physobj, pos, rot, vel, angvel)`: `DMS.BodyState`'s inverse (`SetBodyState`, `VBOX3D_NM_006`):
/// the body teleported there, moving so. False if it's refused (frozen, static), nil before `_006`.
unsafe fn set_body_state_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = nm()?;
    let object = unsafe { api.check_physobj(l, 1)? };
    let rot = unsafe { api.check_vec4(l, 3)? };
    let s = vphysics_nm::PhysicsNMBodyState {
        position: unsafe { api.check_vec3(l, 2)? },
        rotation: rot,
        velocity: unsafe { api.check_vec3(l, 4)? },
        angular_velocity: unsafe { api.check_vec3(l, 5)? },
    };
    // SAFETY: a live PhysObj, from a native (outside Simulate).
    match unsafe { nm.set_body_state(object, &s) } {
        Some(ok) => unsafe { (api.pushboolean)(l, ok.into()) },
        None => unsafe { (api.pushnil)(l) },
    }
    Ok(1)
}

native!(body_inertia => body_inertia_impl);
/// `DMS.BodyInertia(physobj)`: the inertia tensor Box3D turns the body with, 9 numbers row by row (kg m^2, about the
/// centre of mass in the body's frame); nil before `VBOX3D_NM_003`.
unsafe fn body_inertia_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = nm()?;
    let object = unsafe { api.check_physobj(l, 1)? };
    match unsafe { nm.body_inertia(object) } {
        Some(t) => unsafe {
            (api.createtable)(l, 9, 0);
            for (i, v) in t.iter().enumerate() {
                (api.pushnumber)(l, f64::from(*v));
                api.append(l, i + 1);
            }
        },
        None => unsafe { (api.pushnil)(l) },
    }
    Ok(1)
}

native!(drive => drive_impl);
/// `DMS.Drive(physobj, force, torque[, steps = 1])`: force (N) at the centre of mass and torque (Nm), world, over
/// the next `steps` physics steps. True if it was taken.
unsafe fn drive_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = nm()?;
    let object = unsafe { api.check_physobj(l, 1)? };
    let force = unsafe { api.check_vec3(l, 2)? };
    let torque = unsafe { api.check_vec3(l, 3)? };
    let steps = unsafe { api.opt_number(l, 4, 1.0)? };
    if !(steps >= 1.0) {
        return Err("bad argument #4 (steps must be 1 or more)".into());
    }
    let ok = if steps < 2.0 {
        // SAFETY: a live PhysObj; the drive is consumed by the next step.
        unsafe { nm.apply_body_drive(object, Some(&force), Some(&torque)) }
    } else {
        // SAFETY: a live PhysObj; a dead one's drive runs out within `steps` steps, as with any drive in M1.
        unsafe { step::add(object, force, torque, Some(steps as u32)) }.is_ok()
    };
    unsafe { (api.pushboolean)(l, ok.into()) };
    Ok(1)
}

native!(find_joint => find_joint_impl);
/// `DMS.FindJoint(physA, physB)`: whether a constraint joins the two objects.
unsafe fn find_joint_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = nm()?;
    let a = unsafe { api.check_physobj(l, 1)? };
    let b = unsafe { api.check_physobj(l, 2)? };
    let found = !unsafe { nm.find_constraint(a, b) }.is_null();
    unsafe { (api.pushboolean)(l, found.into()) };
    Ok(1)
}

native!(set_armature => set_armature_impl);
/// `DMS.SetArmature(physA, physB, value)`: the armature (kg m^2) of the joint between the two objects, clamped to
/// 1e6. False if there's no such joint or the value is rejected.
unsafe fn set_armature_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = nm()?;
    let a = unsafe { api.check_physobj(l, 1)? };
    let b = unsafe { api.check_physobj(l, 2)? };
    let value = unsafe { api.check_number(l, 3)? }.min(f64::from(MAX_ARMATURE)) as f32;
    let joint = unsafe { nm.find_constraint(a, b) };
    let ok = !joint.is_null() && unsafe { nm.set_joint_armature(joint, value) };
    unsafe { (api.pushboolean)(l, ok.into()) };
    Ok(1)
}

native!(get_armature => get_armature_impl);
/// `DMS.GetArmature(physA, physB)`: the armature of the joint between the two objects, nil for no joint.
unsafe fn get_armature_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = nm()?;
    let a = unsafe { api.check_physobj(l, 1)? };
    let b = unsafe { api.check_physobj(l, 2)? };
    let joint = unsafe { nm.find_constraint(a, b) };
    unsafe {
        if joint.is_null() {
            (api.pushnil)(l);
        } else {
            (api.pushnumber)(l, f64::from(nm.joint_armature(joint)));
        }
    }
    Ok(1)
}

native!(substep_count => substep_count_impl);
/// `DMS.SubstepCount()`: Box3D sub-steps per physics step.
unsafe fn substep_count_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = nm()?;
    unsafe { (api.pushnumber)(l, f64::from(nm.substep_count())) };
    Ok(1)
}

native!(hover => hover_impl);
/// `DMS.Hover(physobj, massKg, gravity)` holds the object up with `massKg * gravity` (m/s^2) newtons every step,
/// `DMS.Hover(physobj, false)` stops it. Call the latter before the object goes.
unsafe fn hover_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    nm()?;
    let object = unsafe { api.check_physobj(l, 1)? };
    if unsafe { (api.type_)(l, 2) } == TBOOLEAN {
        if unsafe { (api.toboolean)(l, 2) } != 0 {
            return Err("bad argument #2 (mass expected to start hovering, false to stop)".into());
        }
        let removed = step::remove(object);
        unsafe { (api.pushboolean)(l, removed.into()) };
        return Ok(1);
    }
    let mass = unsafe { api.check_number(l, 2)? };
    let gravity = unsafe { api.check_number(l, 3)? };
    let up = (mass * gravity) as f32;
    if !up.is_finite() {
        return Err("mass * gravity isn't finite".into());
    }
    // SAFETY: a live PhysObj, which Lua stops hovering before it's removed.
    unsafe { step::add(object, [0.0, 0.0, up], [0.0; 3], None)? };
    unsafe { (api.pushboolean)(l, true.into()) };
    Ok(1)
}

native!(shutdown => shutdown_impl);
/// `DMS.Shutdown()`: drops every drive and removes the step callback, for the `ShutDown` hook.
unsafe fn shutdown_impl(_api: &Api, _l: *mut LuaState) -> Result<c_int, String> {
    step::shutdown();
    Ok(0)
}
