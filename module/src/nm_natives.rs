//! Natives on the `DMS` table for Delirium itself (ROADMAP M2): loading NM's data from the game, the self-test
//! and reference lists (parts, messages and params, the build's parts, its joints).
//!
//! Part indices are NM's (0 Buttocks .. 20 Hand_Right) and Lua lists hold part `i` at `[i + 1]`. Vectors are SI.

use std::ffi::{CStr, c_int};

use crate::data;
use crate::lua::{self, Api, LuaState, native};
use crate::nm::{self, JointKind};
use crate::selftest;

pub const NATIVES: &[(&CStr, lua::CFunction)] = &[
    (c"LoadData", load_data),
    (c"DataInfo", data_info),
    (c"SelfTest", self_test),
    (c"PartNames", part_names),
    (c"Messages", messages),
    (c"MessageParams", message_params),
    (c"BuildParts", build_parts),
    (c"BuildJoints", build_joints),
    (c"BuildDump", build_dump),
];

fn loaded() -> Result<std::sync::Arc<data::Data>, String> {
    data::get().ok_or_else(|| format!("NM's data isn't loaded (DMS.LoadData): {}", data::how_to()))
}

/// Pushes the loaded data's summary.
unsafe fn push_info(api: &Api, l: *mut LuaState, d: &data::Data) {
    unsafe {
        (api.createtable)(l, 0, 6);
        api.set_str(l, c"dir", &d.dir.to_string_lossy());
        api.set_str(l, c"platform", d.platform);
        api.set_number(l, c"parts", d.build.part_count() as f64);
        api.set_str(l, c"taskparams", &d.taskparams);
        api.set_number(l, c"surfaces", d.surfaces as f64);
        api.set_number(l, c"floorMaterial", f64::from(d.floor_material));
    }
}

native!(load_data => load_data_impl);
/// `DMS.LoadData(gameDir)`: loads fred.xrt, taskparams.txt, the surfaces and Homer's build from the game, once.
/// The data's summary (`DMS.DataInfo()`); a Lua error saying what's missing and what to set if it can't.
unsafe fn load_data_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let dir = unsafe { api.check_string(l, 1)? };
    let d = data::load(&dir)?;
    unsafe { push_info(api, l, &d) };
    Ok(1)
}

native!(data_info => data_info_impl);
/// `DMS.DataInfo()`: `{dir, platform, parts, taskparams, surfaces, floorMaterial}`, nil before `LoadData`.
unsafe fn data_info_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    match data::get() {
        Some(d) => unsafe { push_info(api, l, &d) },
        None => unsafe { (api.pushnil)(l) },
    }
    Ok(1)
}

native!(self_test => self_test_impl);
/// `DMS.SelfTest([seconds = 2])`: Homer in LEAD on a flat floor under `bodyBalance`. `{passed, verdict, steps,
/// pelvisStart, pelvisMin, pelvisEnd, headStart, headEnd, balancing, fallen}`.
unsafe fn self_test_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let seconds = unsafe { api.opt_number(l, 1, 2.0)? } as f32;
    let d = loaded()?;
    let r = selftest::run(&d, seconds)?;
    unsafe {
        (api.createtable)(l, 0, 10);
        api.set_bool(l, c"passed", r.passed);
        api.set_str(l, c"verdict", &r.verdict);
        api.set_number(l, c"steps", r.steps as f64);
        for (name, v) in [
            (c"pelvisStart", r.pelvis_start),
            (c"pelvisMin", r.pelvis_min),
            (c"pelvisEnd", r.pelvis_end),
            (c"headStart", r.head_start),
            (c"headEnd", r.head_end),
        ] {
            api.set_number(l, name, f64::from(v));
        }
        api.set_bool(l, c"balancing", r.balancing);
        api.set_bool(l, c"fallen", r.fallen);
    }
    Ok(1)
}

native!(part_names => part_names_impl);
/// `DMS.PartNames()`: NM's 21 part names in part order.
unsafe fn part_names_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let names = (0..nm::PARTS).map(nm::part_name).collect::<Result<Vec<_>, _>>()?;
    unsafe {
        (api.createtable)(l, names.len() as c_int, 0);
        for (i, n) in names.iter().enumerate() {
            api.push_str(l, n);
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

native!(messages => messages_impl);
/// `DMS.Messages()`: every NM message's name.
unsafe fn messages_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let names = (0..nm::message_count()).map(nm::message_name).collect::<Result<Vec<_>, _>>()?;
    unsafe {
        (api.createtable)(l, names.len() as c_int, 0);
        for (i, n) in names.iter().enumerate() {
            api.push_str(l, n);
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

native!(message_params => message_params_impl);
/// `DMS.MessageParams(message)`: its params, `{name, kind, used, doc, default}` each (`default` a number, a
/// `{x, y, z}` or nil).
unsafe fn message_params_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let name = unsafe { api.check_string(l, 1)? };
    let params = nm::message_params(nm::message_find(&name)?)?;
    unsafe {
        (api.createtable)(l, params.len() as c_int, 0);
        for (i, p) in params.iter().enumerate() {
            (api.createtable)(l, 0, 5);
            api.set_str(l, c"name", &p.name);
            api.set_str(l, c"kind", p.kind.name());
            api.set_bool(l, c"used", p.used);
            api.set_str(l, c"doc", &p.doc);
            match p.default[..] {
                [x] => api.set_number(l, c"default", f64::from(x)),
                [_, _, _] => api.set_vector(l, c"default", &p.default),
                _ => {}
            }
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

native!(build_parts => build_parts_impl);
/// `DMS.BuildParts()`: Homer's build, part by part (`nm_build_part_info`, `nm_build_part_frame`): `{index, name,
/// mass, radius, half, parent, pos, rot}` (`parent` -1 for the root; the frame in the build's standing pose).
unsafe fn build_parts_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let d = loaded()?;
    let n = d.build.part_count();
    unsafe { (api.createtable)(l, n as c_int, 0) };
    for i in 0..n {
        let p = d.build.part(i)?;
        let name = nm::part_name(i)?;
        unsafe {
            (api.createtable)(l, 0, 8);
            api.set_number(l, c"index", i as f64);
            api.set_str(l, c"name", &name);
            api.set_number(l, c"mass", f64::from(p.mass));
            api.set_number(l, c"radius", f64::from(p.radius));
            api.set_number(l, c"half", f64::from(p.half));
            api.set_number(l, c"parent", p.parent.map_or(-1.0, |j| j as f64));
            api.set_vector(l, c"pos", &p.pos);
            api.set_vector(l, c"rot", &p.rot);
            api.append(l, i + 1);
        }
    }
    Ok(1)
}

native!(build_joints => build_joints_impl);
/// `DMS.BuildJoints()`: Homer's 20 joints (`nm_agent_get_joint` of an agent in the build's pose at the origin):
/// `{part, parent, kind = "ball" | "hinge", pivot, frame, limits, armature}`.
unsafe fn build_joints_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let d = loaded()?;
    let a = nm::Agent::new(&d.build, [0.0; 3], 0.0, nm::PLAYROOM_SEED)?;
    let joints = (0..d.build.part_count()).map(|i| a.joint(i).map(|j| j.map(|j| (i, j)))).collect::<Result<Vec<_>, _>>()?;
    let joints: Vec<_> = joints.into_iter().flatten().collect();
    unsafe {
        (api.createtable)(l, joints.len() as c_int, 0);
        for (k, (i, j)) in joints.iter().enumerate() {
            (api.createtable)(l, 0, 7);
            api.set_number(l, c"part", *i as f64);
            api.set_number(l, c"parent", j.parent as f64);
            api.set_str(l, c"kind", if j.kind == JointKind::Hinge { "hinge" } else { "ball" });
            api.set_vector(l, c"pivot", &j.pivot);
            api.set_vector(l, c"frame", &j.frame);
            let limits = if j.kind == JointKind::Hinge { &j.limits[..2] } else { &j.limits[..] };
            api.set_vector(l, c"limits", limits);
            api.set_number(l, c"armature", f64::from(j.armature));
            api.append(l, k + 1);
        }
    }
    Ok(1)
}

native!(build_dump => build_dump_impl);
/// `DMS.BuildDump()`: the build as the Playroom's `NM_DUMP` writes it (for `tools/golden`).
unsafe fn build_dump_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    let d = loaded()?;
    unsafe { api.push_str(l, &d.dump) };
    Ok(1)
}
