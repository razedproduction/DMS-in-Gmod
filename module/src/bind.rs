//! Binding Homer's ragdoll (`models/dms/homer.mdl`, `tools/dms-homer`) to NM's body (ROADMAP M3): in the pose
//! it's spawned in, every joint gets NM's pivot (`SetJointAnchor`) and NM's frame and limits (`SetJointLimits`).
//!
//! The ragdoll's physics objects are NM's parts one to one (bones named as `nm_part_name`), each object's frame
//! the part's frame with its centre of mass on the origin, so a body's state is the part's state as it is.

use std::ffi::{CStr, c_int};

use delirium::prelude::*;

use crate::data;
use crate::ffi::vphysics_nm::{self, IPhysicsObject, Nm, Version};
use crate::lua::{self, Api, LuaState, TTABLE, native};

pub const NATIVES: &[(&CStr, lua::CFunction)] = &[(c"BindHomer", bind_homer), (c"HomerBody", homer_body)];

/// The 21 PhysObj of the table at `arg`, `[i + 1]` part `i`'s.
unsafe fn parts(api: &Api, l: *mut LuaState, arg: c_int, count: usize) -> Result<Vec<*mut IPhysicsObject>, String> {
    if unsafe { (api.type_)(l, arg) } != TTABLE {
        return Err(format!("bad argument #{arg} (a table of {count} PhysObj in NM's part order expected)"));
    }
    (0..count)
        .map(|i| unsafe {
            (api.rawgeti)(l, arg, i as c_int + 1);
            let p = api.check_physobj(l, -1).map_err(|_| format!("bad argument #{arg} ([{}] isn't a PhysObj)", i + 1));
            (api.settop)(l, -2);
            p
        })
        .collect()
}

/// A body's frame from its state: the centre of mass is the part's centre (`dms-homer`'s `.phy`).
fn pose(nm: &Nm, object: *mut IPhysicsObject) -> Result<(Vec3, Quat), String> {
    // SAFETY: a PhysObj Lua handed over, alive for this call.
    let s = unsafe { nm.body_state(object) }.ok_or("GetBodyState failed")?;
    Ok((Vec3::from_array(s.position), Quat::from_array(s.rotation).normalize()))
}

native!(bind_homer => bind_homer_impl);
/// `DMS.BindHomer(parts)`: `parts` the ragdoll's 21 PhysObj in NM's part order, standing as it was spawned. Gives
/// every body NM's inertia tensor (`SetBodyInertia`) and every joint NM's pivot, frames and limits (a ball
/// `SetJointBallLimits`, a hinge `SetJointLimits`); `{joint = {part, parent, kind, anchor, limits}, ...}` per
/// joint, or a Lua error naming the part or joint that failed.
unsafe fn bind_homer_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let nm = vphysics_nm::get().ok_or("IPhysicsNM isn't available (vphysics.dll isn't VPhysics-Box3D)")?;
    if nm.version < Version::V004 {
        return Err(format!("{} has no SetBodyMaterial (or SetBodyInertia / SetJointBallLimits): update VPhysics-Box3D", nm.version.name()));
    }
    let d = data::get().ok_or_else(|| format!("NM's data isn't loaded (DMS.LoadData): {}", data::how_to()))?;
    let body = &d.body;
    let objects = unsafe { parts(api, l, 1, body.parts.len())? };
    let poses = objects.iter().map(|&o| pose(&nm, o)).collect::<Result<Vec<_>, _>>()?;
    for (p, &o) in body.parts.iter().zip(&objects) {
        // Symmetric: its columns are its rows.
        let tensor = p.inertia.to_cols_array();
        // SAFETY: a PhysObj of the ragdoll, alive for this call; 9 floats.
        if unsafe { nm.set_body_inertia(o, &tensor) } != Some(true) {
            return Err(format!("{}: SetBodyInertia refused NM's tensor {tensor:?}", p.name));
        }
        // NM's contact material of the part (ROADMAP M5): its friction and elasticity against NM's surfaces.
        let (friction, elasticity) = dms_game::body::part_material(&p.bone);
        // SAFETY: as above.
        if unsafe { nm.set_body_material(o, friction, elasticity) } != Some(true) {
            return Err(format!("{}: SetBodyMaterial refused {friction} {elasticity}", p.name));
        }
    }
    unsafe { (api.createtable)(l, body.joints.len() as c_int, 0) };
    for (k, j) in body.joints.iter().enumerate() {
        let name = body.parts[j.part].name;
        let (parent, child) = (objects[j.parent], objects[j.part]);
        // SAFETY: both are PhysObj of the ragdoll, alive for this call.
        let c = unsafe { nm.find_constraint(parent, child) };
        if c.is_null() {
            return Err(format!("{name}: no constraint to {} (not models/dms/homer.mdl?)", body.parts[j.parent].name));
        }
        let (pivot, frames) = body.joint_world(j, &poses);
        // SAFETY: `c` joins two live objects; the arrays are as long as the calls read.
        let anchored = unsafe { nm.set_joint_anchor(c, Some(&pivot.to_array())) };
        let frames = [frames[0].to_array(), frames[1].to_array()];
        let limited = if j.hinge {
            unsafe { nm.set_joint_limits(c, parent, &frames, true, &j.limits) }
        } else {
            unsafe { nm.set_joint_ball_limits(c, parent, &frames, &j.limits) }
        } == Some(true);
        if !anchored || !limited {
            return Err(format!("{name}: SetJointAnchor {anchored}, {} {limited} (was the constraint built as a {}?)",
                if j.hinge { "SetJointLimits" } else { "SetJointBallLimits" }, if j.hinge { "hinge" } else { "ragdoll constraint" }));
        }
        unsafe {
            (api.createtable)(l, 0, 5);
            api.set_number(l, c"part", j.part as f64);
            api.set_number(l, c"parent", j.parent as f64);
            api.set_str(l, c"kind", if j.hinge { "hinge" } else { "ball" });
            api.set_vector(l, c"anchor", &pivot.to_array());
            api.set_vector(l, c"limits", if j.hinge { &j.limits[..2] } else { &j.limits[..] });
            api.append(l, k + 1);
        }
    }
    Ok(1)
}
native!(homer_body => homer_body_impl);
/// `DMS.HomerBody([pos, yaw])`: NM's body of Homer standing as `nm_agent_create` puts him at `pos` (m, his feet's
/// level; default the origin) facing `yaw` (rad): per part `{name, parent, mass, pos, rot, inertia}` (`inertia`
/// the 3x3 tensor about the centre in the part's frame, row by row, kg m^2).
unsafe fn homer_body_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let d = data::get().ok_or_else(|| format!("NM's data isn't loaded (DMS.LoadData): {}", data::how_to()))?;
    let at = if unsafe { (api.type_)(l, 1) } > lua::TNIL { Vec3::from_array(unsafe { api.check_vec3(l, 1)? }) } else { Vec3::ZERO };
    let yaw = unsafe { api.opt_number(l, 2, 0.0)? } as f32;
    let placed = d.body.placed(at, yaw);
    unsafe { (api.createtable)(l, d.body.parts.len() as c_int, 0) };
    for (i, (p, (pos, rot))) in d.body.parts.iter().zip(placed).enumerate() {
        let t = p.inertia.transpose().to_cols_array();
        unsafe {
            (api.createtable)(l, 0, 6);
            api.set_str(l, c"name", p.name);
            api.set_number(l, c"parent", p.parent.map_or(-1.0, |q| q as f64));
            api.set_number(l, c"mass", f64::from(p.mass));
            api.set_vector(l, c"pos", &pos.to_array());
            api.set_vector(l, c"rot", &rot.to_array());
            (api.createtable)(l, 9, 0);
            for (k, v) in t.iter().enumerate() {
                (api.pushnumber)(l, f64::from(*v));
                api.append(l, k + 1);
            }
            (api.setfield)(l, -2, c"inertia".as_ptr());
            api.append(l, i + 1);
        }
    }
    Ok(1)
}
