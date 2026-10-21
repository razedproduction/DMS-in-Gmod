//! The Delirium SDK (`delirium_physics::nm_*`, linked as an rlib) behind safe wrappers: every call's code is
//! checked, and an `NM_E_*` comes back as an error with `nm_last_error`'s text.

use std::ffi::{CString, c_char};
use std::ptr::NonNull;

use delirium_physics as sdk;

pub const PARTS: usize = 21;

/// Seed and facing of the Playroom's agents (`Agent::new(.., Quat::IDENTITY, 0.3)`).
pub const PLAYROOM_SEED: f32 = 0.3;
/// NM's own step.
pub const DT: f32 = 1.0 / 60.0;

/// `NM_E_*`'s name.
fn code_name(code: i32) -> &'static str {
    match code {
        sdk::NM_E_ARG => "NM_E_ARG",
        sdk::NM_E_IO => "NM_E_IO",
        sdk::NM_E_PARSE => "NM_E_PARSE",
        sdk::NM_E_MESSAGE => "NM_E_MESSAGE",
        sdk::NM_E_PARAM => "NM_E_PARAM",
        sdk::NM_E_TYPE => "NM_E_TYPE",
        sdk::NM_E_STATE => "NM_E_STATE",
        sdk::NM_E_FOLLOW => "NM_E_FOLLOW",
        _ => "NM_E_?",
    }
}

/// `nm_last_error` of this thread.
pub fn last_error() -> String {
    // SAFETY: a null buffer only asks for the length.
    let n = unsafe { sdk::nm_last_error(std::ptr::null_mut(), 0) };
    let mut buf = vec![0u8; n.max(1) as usize];
    // SAFETY: `buf` holds `n` bytes.
    unsafe { sdk::nm_last_error(buf.as_mut_ptr().cast(), buf.len() as i32) };
    text_of(&buf)
}

/// `Ok(code)` for a non-negative code, else the call's error.
fn check(what: &str, code: i32) -> Result<i32, String> {
    if code >= 0 { Ok(code) } else { Err(format!("{what}: {} ({})", last_error(), code_name(code))) }
}

fn c_string(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| format!("{s:?}: a NUL in it"))
}

fn text_of(buf: &[u8]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// A string out of one of the SDK's `(buf, len) -> needed length` calls.
fn string(what: &str, f: impl Fn(*mut c_char, i32) -> i32) -> Result<String, String> {
    let mut buf = vec![0u8; 64];
    loop {
        let n = check(what, f(buf.as_mut_ptr().cast(), buf.len() as i32))? as usize;
        if n <= buf.len() {
            return Ok(text_of(&buf));
        }
        buf.resize(n, 0);
    }
}

/// `nm_init_memory`: fred.xrt's bytes. True if this call loaded it, false if one already had (later calls
/// are ignored).
pub fn init_memory(xrt: &[u8]) -> Result<bool, String> {
    // SAFETY: `xrt` is `len` bytes.
    let r = check("nm_init_memory", unsafe { sdk::nm_init_memory(xrt.as_ptr(), xrt.len() as i32) })?;
    Ok(r == 0)
}

/// `nm_load_task_params` from a file.
pub fn load_task_params(path: &std::path::Path) -> Result<(), String> {
    let p = c_string(&path.to_string_lossy())?;
    // SAFETY: a null terminated path.
    check("nm_load_task_params", unsafe { sdk::nm_load_task_params(p.as_ptr()) }).map(drop)
}

/// NM's part `i`'s name.
pub fn part_name(i: usize) -> Result<String, String> {
    // SAFETY: `buf` holds `len` bytes.
    string("nm_part_name", |buf, len| unsafe { sdk::nm_part_name(i as i32, buf, len) })
}

/// A param's kind (`NM_KIND_*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Float,
    Int,
    Bool,
    Vec3,
    Str,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Float => "float",
            Kind::Int => "int",
            Kind::Bool => "bool",
            Kind::Vec3 => "vec3",
            Kind::Str => "string",
        }
    }
}

/// One of a message's params.
pub struct Param {
    pub name: String,
    pub kind: Kind,
    /// Whether the port acts on it (else it's only kept).
    pub used: bool,
    pub doc: String,
    /// The behaviour's default (1 float, 3 for a vec3, none for a string or a param the port doesn't keep).
    pub default: Vec<f32>,
}

pub fn message_count() -> usize {
    sdk::nm_message_count().max(0) as usize
}

pub fn message_name(m: usize) -> Result<String, String> {
    // SAFETY: `buf` holds `len` bytes.
    string("nm_message_name", |buf, len| unsafe { sdk::nm_message_name(m as i32, buf, len) })
}

/// A message's index by name.
pub fn message_find(name: &str) -> Result<usize, String> {
    let n = c_string(name)?;
    // SAFETY: a null terminated name.
    check("nm_message_find", unsafe { sdk::nm_message_find(n.as_ptr()) }).map(|m| m as usize)
}

/// Message `m`'s params.
pub fn message_params(m: usize) -> Result<Vec<Param>, String> {
    let m = m as i32;
    let count = check("nm_message_param_count", sdk::nm_message_param_count(m))?;
    (0..count)
        .map(|p| {
            let kind = match check("nm_message_param_kind", sdk::nm_message_param_kind(m, p))? {
                sdk::NM_KIND_FLOAT => Kind::Float,
                sdk::NM_KIND_INT => Kind::Int,
                sdk::NM_KIND_BOOL => Kind::Bool,
                sdk::NM_KIND_VEC3 => Kind::Vec3,
                _ => Kind::Str,
            };
            let mut default = [0.0f32; 3];
            // SAFETY: room for a vec3; a param the port doesn't keep fails, which only means no default.
            let n = unsafe { sdk::nm_message_param_default(m, p, default.as_mut_ptr()) };
            Ok(Param {
                // SAFETY: `buf` holds `len` bytes.
                name: string("nm_message_param_name", |buf, len| unsafe { sdk::nm_message_param_name(m, p, buf, len) })?,
                kind,
                used: check("nm_message_param_used", sdk::nm_message_param_used(m, p))? != 0,
                // SAFETY: `buf` holds `len` bytes.
                doc: string("nm_message_param_doc", |buf, len| unsafe { sdk::nm_message_param_doc(m, p, buf, len) })?,
                default: default[..n.clamp(0, 3) as usize].to_vec(),
            })
        })
        .collect()
}

/// A param's value in a message.
#[derive(Clone, Debug)]
pub enum Value {
    Float(f32),
    Int(i32),
    Bool(bool),
    Vec3([f32; 3]),
    Str(String),
}

impl Value {
    /// `text` as a value of `kind`, as the Playroom's console reads it: numbers, `true/false/1/0`, `x,y,z`.
    fn parse(kind: Kind, text: &str) -> Option<Value> {
        Some(match kind {
            Kind::Float => Value::Float(text.parse().ok()?),
            Kind::Int => Value::Int(text.parse().ok()?),
            Kind::Bool => Value::Bool(match text {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => return None,
            }),
            Kind::Vec3 => {
                let v: Vec<f32> = text.split(',').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
                Value::Vec3(v.try_into().ok()?)
            }
            Kind::Str => Value::Str(text.to_string()),
        })
    }
}

/// A message line as the Playroom's console takes it (`send_to`): `<message> [param=value ...]`, `start` a bool,
/// the rest by their kind.
pub fn parse_message(line: &str) -> Result<(String, Vec<(String, Value)>), String> {
    let mut w = line.split_whitespace();
    let name = w.next().ok_or("an empty message")?;
    let params = message_params(message_find(name)?)?;
    let values = w
        .map(|kv| {
            let (k, v) = kv.split_once('=').ok_or_else(|| format!("{kv}: want name=value"))?;
            let kind = if k == "start" { Some(Kind::Bool) } else { params.iter().find(|p| p.name == k).map(|p| p.kind) };
            let kind = kind.ok_or_else(|| format!("{name} has no param {k}"))?;
            let value = Value::parse(kind, v).ok_or_else(|| format!("{k}: not a {}", kind.name()))?;
            Ok((k.to_string(), value))
        })
        .collect::<Result<_, String>>()?;
    Ok((name.to_string(), values))
}

/// `NmWorld`: the geometry NM sees.
pub struct World(NonNull<sdk::NmWorld>);

impl World {
    pub fn new() -> World {
        // SAFETY: never null (a Box).
        World(NonNull::new(sdk::nm_world_create()).expect("nm_world_create"))
    }

    fn ptr(&self) -> *mut sdk::NmWorld {
        self.0.as_ptr()
    }

    /// `nm_world_add_mesh`: one material byte per triangle.
    pub fn add_mesh(&mut self, positions: &[[f32; 3]], triangles: &[[u32; 3]], materials: &[u8]) -> Result<(), String> {
        if materials.len() != triangles.len() {
            return Err("nm_world_add_mesh: one material per triangle".into());
        }
        // SAFETY: the arrays hold `vertices` and `count` items.
        check("nm_world_add_mesh", unsafe {
            sdk::nm_world_add_mesh(
                self.ptr(),
                positions.as_ptr().cast(),
                positions.len() as i32,
                triangles.as_ptr().cast(),
                triangles.len() as i32,
                materials.as_ptr(),
            )
        })
        .map(drop)
    }

}

impl Drop for World {
    fn drop(&mut self) {
        // SAFETY: made by nm_world_create, dropped once.
        unsafe { sdk::nm_world_destroy(self.ptr()) }
    }
}

/// One part of a build.
pub struct PartInfo {
    pub mass: f32,
    pub radius: f32,
    pub half: f32,
    pub parent: Option<usize>,
    /// Its frame in the build's pose: position, rotation (x, y, z, w).
    pub pos: [f32; 3],
    pub rot: [f32; 4],
}

/// `NmBuild`: a body (Homer's).
pub struct Build(NonNull<sdk::NmBuild>);

// SAFETY: a build isn't changed after it's made; the SDK's calls on it only read it.
unsafe impl Send for Build {}
// SAFETY: as above.
unsafe impl Sync for Build {}

impl Build {
    /// `nm_build_from_text`: a build dump (`Build::dump`, the Playroom's `NM_DUMP`).
    pub fn from_text(dump: &str) -> Result<Build, String> {
        let t = c_string(dump)?;
        // SAFETY: a null terminated text.
        let b = unsafe { sdk::nm_build_from_text(t.as_ptr()) };
        NonNull::new(b).map(Build).ok_or_else(|| format!("nm_build_from_text: {}", last_error()))
    }

    fn ptr(&self) -> *const sdk::NmBuild {
        self.0.as_ptr()
    }

    pub fn part_count(&self) -> usize {
        // SAFETY: a live build.
        unsafe { sdk::nm_build_part_count(self.ptr()) }.max(0) as usize
    }

    /// `nm_build_part_info` and `nm_build_part_frame` of part `i`.
    pub fn part(&self, i: usize) -> Result<PartInfo, String> {
        let (mut mass, mut radius, mut half, mut parent) = (0.0, 0.0, 0.0, -1);
        let (mut pos, mut rot) = ([0.0; 3], [0.0; 4]);
        // SAFETY: a live build; the outputs are as big as the call writes.
        unsafe {
            check("nm_build_part_info", sdk::nm_build_part_info(self.ptr(), i as i32, &mut mass, &mut radius, &mut half, &mut parent))?;
            check("nm_build_part_frame", sdk::nm_build_part_frame(self.ptr(), i as i32, pos.as_mut_ptr(), rot.as_mut_ptr()))?;
        }
        Ok(PartInfo { mass, radius, half, parent: usize::try_from(parent).ok(), pos, rot })
    }
}

impl Drop for Build {
    fn drop(&mut self) {
        // SAFETY: made by nm_build_from_text, dropped once.
        unsafe { sdk::nm_build_destroy(self.0.as_ptr()) }
    }
}

/// A joint's kind (`nm_agent_get_joint`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JointKind {
    Ball,
    Hinge,
}

/// Part `i`'s joint to its parent, as NM's physics has it.
pub struct Joint {
    pub kind: JointKind,
    pub parent: usize,
    pub pivot: [f32; 3],
    pub frame: [f32; 4],
    /// A ball's lean X, lean Y and twist half ranges, a hinge's low and high (rad).
    pub limits: [f32; 3],
    pub armature: f32,
}

/// NM's run mode. FOLLOW comes with the step loop (M4).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Lead,
    Follow,
}

/// `NmAgent`.
pub struct Agent(NonNull<sdk::NmAgent>);

impl Agent {
    /// `nm_agent_create`: standing at `pos` (its feet's level), facing `yaw` from the build's pose.
    pub fn new(build: &Build, pos: [f32; 3], yaw: f32, seed: f32) -> Result<Agent, String> {
        // SAFETY: a live build, a 3 float position.
        let a = unsafe { sdk::nm_agent_create(build.ptr(), pos.as_ptr(), yaw, seed) };
        NonNull::new(a).map(Agent).ok_or_else(|| format!("nm_agent_create: {}", last_error()))
    }

    fn ptr(&self) -> *mut sdk::NmAgent {
        self.0.as_ptr()
    }

    pub fn set_mode(&mut self, mode: Mode) -> Result<(), String> {
        let m = match mode {
            Mode::Lead => sdk::NM_MODE_LEAD,
            Mode::Follow => sdk::NM_MODE_FOLLOW,
        };
        // SAFETY: a live agent.
        check("nm_agent_set_mode", unsafe { sdk::nm_agent_set_mode(self.ptr(), m) }).map(drop)
    }

    /// `nm_agent_step` of `dt` seconds.
    pub fn step(&mut self, world: &World, dt: f32) -> Result<(), String> {
        // SAFETY: a live agent and world.
        check("nm_agent_step", unsafe { sdk::nm_agent_step(self.ptr(), world.ptr(), dt) }).map(drop)
    }

    /// Every part's frame: position and rotation (x, y, z, w).
    pub fn part_transforms(&self) -> Result<Vec<([f32; 3], [f32; 4])>, String> {
        let mut out = [0.0f32; PARTS * 7];
        // SAFETY: room for PARTS frames.
        let n = check("nm_agent_get_part_transforms", unsafe {
            sdk::nm_agent_get_part_transforms(self.ptr(), out.as_mut_ptr(), PARTS as i32)
        })? as usize;
        Ok(out.chunks(7).take(n).map(|c| ([c[0], c[1], c[2]], [c[3], c[4], c[5], c[6]])).collect())
    }

    /// The body's centre of mass and its velocity (`nm_agent_get_com`, the Playroom's `doll.com()`).
    pub fn com(&self) -> Result<([f32; 3], [f32; 3]), String> {
        let (mut x, mut v) = ([0.0; 3], [0.0; 3]);
        // SAFETY: a live agent, two 3 float outputs.
        check("nm_agent_get_com", unsafe { sdk::nm_agent_get_com(self.ptr(), x.as_mut_ptr(), v.as_mut_ptr()) })?;
        Ok((x, v))
    }

    /// Part `i`'s contact flag (`nm_agent_get_part_contact`).
    pub fn part_contact(&self, i: usize) -> Result<u8, String> {
        // SAFETY: a live agent.
        check("nm_agent_get_part_contact", unsafe { sdk::nm_agent_get_part_contact(self.ptr(), i as i32) }).map(|c| c as u8)
    }

    /// Every part's drive: force and torque (`nm_agent_get_part_drives`).
    pub fn part_drives(&self) -> Result<Vec<([f32; 3], [f32; 3])>, String> {
        let mut out = [0.0f32; PARTS * 6];
        // SAFETY: room for PARTS drives.
        let n = check("nm_agent_get_part_drives", unsafe {
            sdk::nm_agent_get_part_drives(self.ptr(), out.as_mut_ptr(), PARTS as i32)
        })? as usize;
        Ok(out.chunks(6).take(n).map(|c| ([c[0], c[1], c[2]], [c[3], c[4], c[5]])).collect())
    }

    /// This step's feedback: name and slot.
    pub fn feedback(&self) -> Result<Vec<(String, i32)>, String> {
        // SAFETY: a live agent.
        let n = unsafe { sdk::nm_agent_feedback_count(self.ptr()) }.max(0);
        (0..n)
            .map(|i| {
                let mut slot = 0;
                let at: *mut i32 = &mut slot;
                // SAFETY: a live agent; `buf` holds `len` bytes, `at` is one int.
                let name = string("nm_agent_feedback_get", |buf, len| unsafe {
                    sdk::nm_agent_feedback_get(self.ptr(), i, buf, len, at, std::ptr::null_mut())
                })?;
                Ok((name, slot))
            })
            .collect()
    }

    pub fn is_balancing(&self) -> Result<bool, String> {
        // SAFETY: a live agent.
        check("nm_agent_is_balancing", unsafe { sdk::nm_agent_is_balancing(self.ptr()) }).map(|r| r != 0)
    }

    pub fn has_fallen(&self) -> Result<bool, String> {
        // SAFETY: a live agent.
        check("nm_agent_has_fallen", unsafe { sdk::nm_agent_has_fallen(self.ptr()) }).map(|r| r != 0)
    }

    /// Part `i`'s joint to its parent, none for the root.
    pub fn joint(&self, i: usize) -> Result<Option<Joint>, String> {
        let mut parent = -1;
        let (mut pivot, mut frame, mut limits, mut armature) = ([0.0; 3], [0.0; 4], [0.0; 3], 0.0);
        // SAFETY: a live agent; the outputs are as big as the call writes.
        let kind = check("nm_agent_get_joint", unsafe {
            sdk::nm_agent_get_joint(
                self.ptr(),
                i as i32,
                &mut parent,
                pivot.as_mut_ptr(),
                frame.as_mut_ptr(),
                limits.as_mut_ptr(),
                &mut armature,
            )
        })?;
        let kind = match kind {
            0 => return Ok(None),
            2 => JointKind::Hinge,
            _ => JointKind::Ball,
        };
        Ok(Some(Joint { kind, parent: parent.max(0) as usize, pivot, frame, limits, armature }))
    }

    /// Sends `message` with `params` (`nm_msg_begin`, `nm_msg_set_*`, `nm_msg_send`); "start" is a param like
    /// any other. Returns how many params the port took without acting on them.
    pub fn send(&mut self, world: &World, message: &str, params: &[(&str, Value)]) -> Result<usize, String> {
        let m = c_string(message)?;
        // SAFETY: a live agent, a null terminated name.
        check("nm_msg_begin", unsafe { sdk::nm_msg_begin(self.ptr(), m.as_ptr()) })?;
        let r = self.add_params(params);
        if let Err(e) = r {
            // SAFETY: a live agent.
            unsafe { sdk::nm_msg_cancel(self.ptr()) };
            return Err(format!("{message}: {e}"));
        }
        // SAFETY: a live agent and world.
        check(message, unsafe { sdk::nm_msg_send(self.ptr(), world.ptr()) }).map(|n| n as usize)
    }

    fn add_params(&mut self, params: &[(&str, Value)]) -> Result<(), String> {
        for (name, v) in params {
            let n = c_string(name)?;
            let a = self.ptr();
            // SAFETY: a live agent, a null terminated name, values that outlive the call.
            let r = unsafe {
                match v {
                    Value::Float(x) => sdk::nm_msg_set_float(a, n.as_ptr(), *x),
                    Value::Int(i) => sdk::nm_msg_set_int(a, n.as_ptr(), *i),
                    Value::Bool(b) => sdk::nm_msg_set_bool(a, n.as_ptr(), *b as i32),
                    Value::Vec3(v) => sdk::nm_msg_set_vec3(a, n.as_ptr(), v.as_ptr()),
                    Value::Str(s) => {
                        let s = c_string(s)?;
                        sdk::nm_msg_set_string(a, n.as_ptr(), s.as_ptr())
                    }
                }
            };
            check(name, r)?;
        }
        Ok(())
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        // SAFETY: made by nm_agent_create, dropped once.
        unsafe { sdk::nm_agent_destroy(self.ptr()) }
    }
}
