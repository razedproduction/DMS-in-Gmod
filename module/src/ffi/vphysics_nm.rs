//! VPhysics-Box3D's `IPhysicsNM` (`VBOX3D_NM_006`), as `include/vphysics_nm.h` of the fork declares it (a copy
//! is beside this file).
//!
//! Win64 only: a C++ virtual call is a plain C call with `this` first, so the vtable is a struct of
//! `extern "C"` function pointers in declaration order.

use std::ffi::{c_char, c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

/// `IPhysicsObject`, only ever handled by pointer.
#[repr(C)]
pub struct IPhysicsObject {
    _private: [u8; 0],
}

/// `IPhysicsConstraint`, only ever handled by pointer.
#[repr(C)]
pub struct IPhysicsConstraint {
    _private: [u8; 0],
}

/// `IPhysicsEnvironment`, only ever handled by pointer.
#[repr(C)]
pub struct IPhysicsEnvironment {
    _private: [u8; 0],
}

pub const INTERFACE_VERSION: &str = "VBOX3D_NM_006";
pub const INTERFACE_VERSION_005: &str = "VBOX3D_NM_005";
pub const INTERFACE_VERSION_004: &str = "VBOX3D_NM_004";
pub const INTERFACE_VERSION_003: &str = "VBOX3D_NM_003";
pub const INTERFACE_VERSION_002: &str = "VBOX3D_NM_002";
pub const INTERFACE_VERSION_001: &str = "VBOX3D_NM_001";

/// Armature above this (kg m^2) is clamped to it.
pub const MAX_ARMATURE: f32 = 1.0e6;

/// `PhysicsNMBodyState_t`: a body's state in SI units, world frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicsNMBodyState {
    /// Centre of mass, m.
    pub position: [f32; 3],
    /// Body frame to world, quaternion x, y, z, w.
    pub rotation: [f32; 4],
    /// Centre of mass velocity, m/s.
    pub velocity: [f32; 3],
    /// rad/s.
    pub angular_velocity: [f32; 3],
}

/// `PhysicsNMStepFn_t`.
pub type StepFn = unsafe extern "C" fn(env: *mut IPhysicsEnvironment, timestep: f32, context: *mut c_void);

/// `IPhysicsNM`: an object whose first field is its vtable.
#[repr(C)]
pub struct IPhysicsNM {
    pub vtable: *const IPhysicsNMVtbl,
}

/// The vtable of `IPhysicsNM`, strictly in the header's declaration order (no virtual destructor).
#[repr(C)]
pub struct IPhysicsNMVtbl {
    pub get_body_state:
        unsafe extern "C" fn(this: *mut IPhysicsNM, object: *const IPhysicsObject, state: *mut PhysicsNMBodyState) -> bool,
    pub apply_body_drive:
        unsafe extern "C" fn(this: *mut IPhysicsNM, object: *mut IPhysicsObject, force: *const f32, torque: *const f32) -> bool,
    pub set_joint_armature:
        unsafe extern "C" fn(this: *mut IPhysicsNM, constraint: *mut IPhysicsConstraint, armature: f32) -> bool,
    pub get_joint_armature: unsafe extern "C" fn(this: *mut IPhysicsNM, constraint: *const IPhysicsConstraint) -> f32,
    pub find_constraint: unsafe extern "C" fn(
        this: *mut IPhysicsNM,
        object0: *mut IPhysicsObject,
        object1: *mut IPhysicsObject,
    ) -> *mut IPhysicsConstraint,
    pub set_step_callback: unsafe extern "C" fn(
        this: *mut IPhysicsNM,
        env: *mut IPhysicsEnvironment,
        callback: Option<StepFn>,
        context: *mut c_void,
    ) -> bool,
    pub get_object_environment:
        unsafe extern "C" fn(this: *mut IPhysicsNM, object: *const IPhysicsObject) -> *mut IPhysicsEnvironment,
    pub get_substep_count: unsafe extern "C" fn(this: *mut IPhysicsNM) -> c_int,
    pub set_joint_anchor:
        unsafe extern "C" fn(this: *mut IPhysicsNM, constraint: *mut IPhysicsConstraint, world_point: *const f32) -> bool,
    /// From `VBOX3D_NM_002` on: an `_001` vtable ends before it.
    pub set_joint_limits: unsafe extern "C" fn(
        this: *mut IPhysicsNM,
        constraint: *mut IPhysicsConstraint,
        parent: *mut IPhysicsObject,
        world_frames: *const f32,
        hinge: bool,
        limits: *const f32,
    ) -> bool,
    /// From `VBOX3D_NM_003` on: an `_002` vtable ends before them.
    pub set_body_inertia: unsafe extern "C" fn(this: *mut IPhysicsNM, object: *mut IPhysicsObject, tensor: *const f32) -> bool,
    pub set_joint_ball_limits: unsafe extern "C" fn(
        this: *mut IPhysicsNM,
        constraint: *mut IPhysicsConstraint,
        parent: *mut IPhysicsObject,
        world_frames: *const f32,
        limits: *const f32,
    ) -> bool,
    pub get_body_inertia: unsafe extern "C" fn(this: *mut IPhysicsNM, object: *const IPhysicsObject, tensor: *mut f32) -> bool,
    /// From `VBOX3D_NM_004` on: an `_003` vtable ends before them.
    pub set_body_material: unsafe extern "C" fn(this: *mut IPhysicsNM, object: *mut IPhysicsObject, friction: f32, elasticity: f32) -> bool,
    pub set_surface_material: unsafe extern "C" fn(this: *mut IPhysicsNM, surface_prop: c_int, friction: f32, elasticity: f32) -> bool,
    /// From `VBOX3D_NM_005` on: an `_004` vtable ends before them.
    pub create_body: unsafe extern "C" fn(
        this: *mut IPhysicsNM,
        env: *mut IPhysicsEnvironment,
        owner: *mut IPhysicsObject,
        points: *const f32,
        count: c_int,
        mass: f32,
        position: *const f32,
        rotation: *const f32,
        game_flags: u16,
    ) -> *mut IPhysicsObject,
    pub create_joint:
        unsafe extern "C" fn(this: *mut IPhysicsNM, parent: *mut IPhysicsObject, child: *mut IPhysicsObject, hinge: bool) -> *mut IPhysicsConstraint,
    pub destroy_joint: unsafe extern "C" fn(this: *mut IPhysicsNM, constraint: *mut IPhysicsConstraint),
    pub destroy_body: unsafe extern "C" fn(this: *mut IPhysicsNM, object: *mut IPhysicsObject),
    /// From `VBOX3D_NM_006` on: a `_005` vtable ends before them.
    pub set_body_collision_pairs:
        unsafe extern "C" fn(this: *mut IPhysicsNM, objects: *const *mut IPhysicsObject, count: c_int, pairs: *const c_int, pair_count: c_int) -> bool,
    pub set_body_state: unsafe extern "C" fn(this: *mut IPhysicsNM, object: *mut IPhysicsObject, state: *const PhysicsNMBodyState) -> bool,
}

/// Which version of the interface the loaded vphysics gave.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    V001 = 1,
    V002 = 2,
    V003 = 3,
    V004 = 4,
    V005 = 5,
    V006 = 6,
}

impl Version {
    pub fn name(self) -> &'static str {
        match self {
            Version::V001 => INTERFACE_VERSION_001,
            Version::V002 => INTERFACE_VERSION_002,
            Version::V003 => INTERFACE_VERSION_003,
            Version::V004 => INTERFACE_VERSION_004,
            Version::V005 => INTERFACE_VERSION_005,
            Version::V006 => INTERFACE_VERSION,
        }
    }
}

/// Why there's no interface.
#[derive(Debug)]
pub enum AcquireError {
    /// No `vphysics.dll` in the process (or no `CreateInterface` in it).
    NoVphysics,
    /// The loaded vphysics isn't VPhysics-Box3D with the NM extension.
    NotBox3d,
}

static NM: AtomicPtr<IPhysicsNM> = AtomicPtr::new(ptr::null_mut());
static VERSION: AtomicU8 = AtomicU8::new(0);

/// Asks the loaded `vphysics.dll` for `IPhysicsNM`, newest version first, and keeps it for [`get`].
pub fn acquire() -> Result<Version, AcquireError> {
    release();
    let create = sys::create_interface().ok_or(AcquireError::NoVphysics)?;
    for (version, name) in [(Version::V006, c"VBOX3D_NM_006"), (Version::V005, c"VBOX3D_NM_005"), (Version::V004, c"VBOX3D_NM_004"), (Version::V003, c"VBOX3D_NM_003"), (Version::V002, c"VBOX3D_NM_002"), (Version::V001, c"VBOX3D_NM_001")] {
        // SAFETY: vphysics' exported factory, given a null terminated name and no return code.
        let p = unsafe { create(name.as_ptr(), ptr::null_mut()) }.cast::<IPhysicsNM>();
        if !p.is_null() {
            VERSION.store(version as u8, Ordering::Release);
            NM.store(p, Ordering::Release);
            return Ok(version);
        }
    }
    Err(AcquireError::NotBox3d)
}

/// Forgets the interface.
pub fn release() {
    NM.store(ptr::null_mut(), Ordering::Release);
    VERSION.store(0, Ordering::Release);
}

/// The interface [`acquire`] found, if any.
pub fn get() -> Option<Nm> {
    let p = NM.load(Ordering::Acquire);
    let version = match VERSION.load(Ordering::Acquire) {
        1 => Version::V001,
        2 => Version::V002,
        3 => Version::V003,
        4 => Version::V004,
        5 => Version::V005,
        6 => Version::V006,
        _ => return None,
    };
    (!p.is_null()).then_some(Nm { this: p, version })
}

/// A live `IPhysicsNM`. Every call must come from the thread that runs the physics (the game's main thread).
#[derive(Clone, Copy)]
pub struct Nm {
    this: *mut IPhysicsNM,
    pub version: Version,
}

impl Nm {
    fn vtbl(&self) -> &IPhysicsNMVtbl {
        // SAFETY: `this` is the interface vphysics handed out, which lives as long as the DLL does.
        unsafe { &*(*self.this).vtable }
    }

    /// # Safety
    /// `object` is null or a live `IPhysicsObject` of this vphysics.
    pub unsafe fn body_state(&self, object: *const IPhysicsObject) -> Option<PhysicsNMBodyState> {
        let mut state = PhysicsNMBodyState::default();
        unsafe { (self.vtbl().get_body_state)(self.this, object, &mut state) }.then_some(state)
    }

    /// # Safety
    /// `object` is null or a live `IPhysicsObject` of this vphysics.
    pub unsafe fn apply_body_drive(&self, object: *mut IPhysicsObject, force: Option<&[f32; 3]>, torque: Option<&[f32; 3]>) -> bool {
        let force = force.map_or(ptr::null(), |f| f.as_ptr());
        let torque = torque.map_or(ptr::null(), |t| t.as_ptr());
        unsafe { (self.vtbl().apply_body_drive)(self.this, object, force, torque) }
    }

    /// # Safety
    /// `constraint` is null or a live `IPhysicsConstraint` of this vphysics.
    pub unsafe fn set_joint_armature(&self, constraint: *mut IPhysicsConstraint, armature: f32) -> bool {
        unsafe { (self.vtbl().set_joint_armature)(self.this, constraint, armature) }
    }

    /// # Safety
    /// `constraint` is null or a live `IPhysicsConstraint` of this vphysics.
    pub unsafe fn joint_armature(&self, constraint: *const IPhysicsConstraint) -> f32 {
        unsafe { (self.vtbl().get_joint_armature)(self.this, constraint) }
    }

    /// # Safety
    /// Both objects are null or live `IPhysicsObject`s of this vphysics.
    pub unsafe fn find_constraint(&self, a: *mut IPhysicsObject, b: *mut IPhysicsObject) -> *mut IPhysicsConstraint {
        unsafe { (self.vtbl().find_constraint)(self.this, a, b) }
    }

    /// # Safety
    /// `env` is null or a live environment; `context` outlives the callback's registration.
    pub unsafe fn set_step_callback(&self, env: *mut IPhysicsEnvironment, callback: Option<StepFn>, context: *mut c_void) -> bool {
        unsafe { (self.vtbl().set_step_callback)(self.this, env, callback, context) }
    }

    /// # Safety
    /// `object` is null or a live `IPhysicsObject` of this vphysics.
    pub unsafe fn object_environment(&self, object: *const IPhysicsObject) -> *mut IPhysicsEnvironment {
        unsafe { (self.vtbl().get_object_environment)(self.this, object) }
    }

    pub fn substep_count(&self) -> i32 {
        // SAFETY: no arguments beyond the live interface.
        unsafe { (self.vtbl().get_substep_count)(self.this) }
    }

    /// # Safety
    /// `constraint` is null or a live `IPhysicsConstraint` of this vphysics.
    pub unsafe fn set_joint_anchor(&self, constraint: *mut IPhysicsConstraint, world_point: Option<&[f32; 3]>) -> bool {
        let point = world_point.map_or(ptr::null(), |p| p.as_ptr());
        unsafe { (self.vtbl().set_joint_anchor)(self.this, constraint, point) }
    }

    /// `None` on `VBOX3D_NM_001`, whose vtable has no such slot.
    ///
    /// # Safety
    /// `constraint` and `parent` are null or live objects of this vphysics.
    pub unsafe fn set_joint_limits(
        &self,
        constraint: *mut IPhysicsConstraint,
        parent: *mut IPhysicsObject,
        world_frames: &[[f32; 4]; 2],
        hinge: bool,
        limits: &[f32; 3],
    ) -> Option<bool> {
        if self.version == Version::V001 {
            return None;
        }
        let frames = world_frames.as_ptr().cast::<f32>();
        Some(unsafe { (self.vtbl().set_joint_limits)(self.this, constraint, parent, frames, hinge, limits.as_ptr()) })
    }

    /// `SetBodyInertia`: `tensor` row by row about the centre of mass in the body's frame (kg m^2). `None` before
    /// `VBOX3D_NM_003`.
    ///
    /// # Safety
    /// `object` is null or a live `IPhysicsObject` of this vphysics.
    pub unsafe fn set_body_inertia(&self, object: *mut IPhysicsObject, tensor: &[f32; 9]) -> Option<bool> {
        if self.version < Version::V003 {
            return None;
        }
        Some(unsafe { (self.vtbl().set_body_inertia)(self.this, object, tensor.as_ptr()) })
    }

    /// `SetJointBallLimits`: NM's ball (lean about the parent frame's Y and X, twist about Z). `None` before
    /// `VBOX3D_NM_003`.
    ///
    /// # Safety
    /// `constraint` and `parent` are null or live objects of this vphysics.
    pub unsafe fn set_joint_ball_limits(
        &self,
        constraint: *mut IPhysicsConstraint,
        parent: *mut IPhysicsObject,
        world_frames: &[[f32; 4]; 2],
        limits: &[f32; 3],
    ) -> Option<bool> {
        if self.version < Version::V003 {
            return None;
        }
        let frames = world_frames.as_ptr().cast::<f32>();
        Some(unsafe { (self.vtbl().set_joint_ball_limits)(self.this, constraint, parent, frames, limits.as_ptr()) })
    }

    /// `GetBodyInertia`: the tensor the solver turns the body with, row by row (kg m^2, body frame). `None` before
    /// `VBOX3D_NM_003` or for a null object.
    ///
    /// # Safety
    /// `object` is null or a live `IPhysicsObject` of this vphysics.
    pub unsafe fn body_inertia(&self, object: *const IPhysicsObject) -> Option<[f32; 9]> {
        if self.version < Version::V003 {
            return None;
        }
        let mut t = [0.0f32; 9];
        unsafe { (self.vtbl().get_body_inertia)(self.this, object, t.as_mut_ptr()) }.then_some(t)
    }

    /// `SetBodyMaterial`: the friction and elasticity NM's physics gives the part, for its contacts with NM's
    /// surfaces and parts. `None` before `VBOX3D_NM_004`.
    ///
    /// # Safety
    /// `object` is null or a live `IPhysicsObject` of this vphysics.
    pub unsafe fn set_body_material(&self, object: *mut IPhysicsObject, friction: f32, elasticity: f32) -> Option<bool> {
        if self.version < Version::V004 {
            return None;
        }
        Some(unsafe { (self.vtbl().set_body_material)(self.this, object, friction, elasticity) })
    }

    /// `SetSurfaceMaterial`: NM's friction and elasticity of a Source surface property (a negative friction takes it
    /// back). `None` before `VBOX3D_NM_004`.
    pub fn set_surface_material(&self, surface_prop: i32, friction: f32, elasticity: f32) -> Option<bool> {
        if self.version < Version::V004 {
            return None;
        }
        // SAFETY: plain values on the live interface.
        Some(unsafe { (self.vtbl().set_surface_material)(self.this, surface_prop, friction, elasticity) })
    }

    /// `CreateBody`: a moving convex hull of `points` (m, the body's frame) of `mass` kg at `position` (m) turned by
    /// `rotation`, standing in for `owner` (its game data, material and flags). `None` before `VBOX3D_NM_005`, null
    /// if it fails.
    ///
    /// # Safety
    /// `env` is a live environment and `owner` null or a live object of it; not from the step callback.
    pub unsafe fn create_body(
        &self,
        env: *mut IPhysicsEnvironment,
        owner: *mut IPhysicsObject,
        points: &[[f32; 3]],
        mass: f32,
        position: &[f32; 3],
        rotation: &[f32; 4],
        game_flags: u16,
    ) -> Option<*mut IPhysicsObject> {
        if self.version < Version::V005 {
            return None;
        }
        let count = points.len() as c_int;
        Some(unsafe {
            (self.vtbl().create_body)(self.this, env, owner, points.as_ptr().cast(), count, mass, position.as_ptr(), rotation.as_ptr(), game_flags)
        })
    }

    /// `CreateJoint`: a ragdoll constraint from `parent` to `child` as they are posed now (a hinge or a ball), wide
    /// open till its limits are set. `None` before `VBOX3D_NM_005`.
    ///
    /// # Safety
    /// Both are live objects of one environment; not from the step callback.
    pub unsafe fn create_joint(&self, parent: *mut IPhysicsObject, child: *mut IPhysicsObject, hinge: bool) -> Option<*mut IPhysicsConstraint> {
        if self.version < Version::V005 {
            return None;
        }
        Some(unsafe { (self.vtbl().create_joint)(self.this, parent, child, hinge) })
    }

    /// `DestroyJoint` (a no-op before `VBOX3D_NM_005`).
    ///
    /// # Safety
    /// `constraint` is null or one `create_joint` made, not destroyed yet.
    pub unsafe fn destroy_joint(&self, constraint: *mut IPhysicsConstraint) {
        if self.version >= Version::V005 {
            unsafe { (self.vtbl().destroy_joint)(self.this, constraint) }
        }
    }

    /// `DestroyBody` (a no-op before `VBOX3D_NM_005`).
    ///
    /// # Safety
    /// `object` is null or one `create_body` made, its joints destroyed.
    pub unsafe fn destroy_body(&self, object: *mut IPhysicsObject) {
        if self.version >= Version::V005 {
            unsafe { (self.vtbl().destroy_body)(self.this, object) }
        }
    }

    /// `SetBodyCollisionPairs`: `objects` one body for their own collisions, only `pairs` (indices into `objects`)
    /// colliding. `None` before `VBOX3D_NM_006`.
    ///
    /// # Safety
    /// The objects are live; not from the step callback.
    pub unsafe fn set_body_collision_pairs(&self, objects: &[*mut IPhysicsObject], pairs: &[(usize, usize)]) -> Option<bool> {
        if self.version < Version::V006 {
            return None;
        }
        let flat: Vec<c_int> = pairs.iter().flat_map(|&(a, b)| [a as c_int, b as c_int]).collect();
        Some(unsafe { (self.vtbl().set_body_collision_pairs)(self.this, objects.as_ptr(), objects.len() as c_int, flat.as_ptr(), pairs.len() as c_int) })
    }

    /// `SetBodyState`: the body teleported to `state` (centre of mass, rotation, velocities; SI, world). `None` before
    /// `VBOX3D_NM_006`.
    ///
    /// # Safety
    /// `object` is null or a live `IPhysicsObject` of this vphysics; not from the step callback.
    pub unsafe fn set_body_state(&self, object: *mut IPhysicsObject, state: &PhysicsNMBodyState) -> Option<bool> {
        if self.version < Version::V006 {
            return None;
        }
        Some(unsafe { (self.vtbl().set_body_state)(self.this, object, state) })
    }
}

/// `IPhysicsEnvironment`'s step (Source SDK 2013 `vphysics_interface.h`: slot 36 `GetSimulationTimestep`, 37
/// `SetSimulationTimestep`, the virtual destructor's one slot first): the game sets 0.015 s, whatever the tickrate.
pub mod env {
    use super::IPhysicsEnvironment;

    const GET_TIMESTEP: usize = 36;
    const SET_TIMESTEP: usize = 37;

    unsafe fn slot(env: *mut IPhysicsEnvironment, i: usize) -> *const () {
        // SAFETY: a live C++ object, its first field the vtable (the caller's promise); the slot is in it.
        unsafe { *(*(env as *const *const *const ())).add(i) }
    }

    /// The environment's physics step (s).
    ///
    /// # Safety
    /// `env` is a live `IPhysicsEnvironment` of this vphysics, called from the game's main thread.
    pub unsafe fn timestep(env: *mut IPhysicsEnvironment) -> f32 {
        // SAFETY: `float GetSimulationTimestep() const`, `this` first.
        let f: unsafe extern "C" fn(*mut IPhysicsEnvironment) -> f32 = unsafe { std::mem::transmute(slot(env, GET_TIMESTEP)) };
        unsafe { f(env) }
    }

    /// Sets the environment's physics step (s). Not from inside its step.
    ///
    /// # Safety
    /// As [`timestep`], outside `Simulate`.
    pub unsafe fn set_timestep(env: *mut IPhysicsEnvironment, t: f32) {
        // SAFETY: `void SetSimulationTimestep(float)`, `this` first.
        let f: unsafe extern "C" fn(*mut IPhysicsEnvironment, f32) = unsafe { std::mem::transmute(slot(env, SET_TIMESTEP)) };
        unsafe { f(env, t) }
    }
}

/// vphysics' `CreateInterface(const char *, int *)`.
type CreateInterfaceFn = unsafe extern "C" fn(name: *const c_char, return_code: *mut c_int) -> *mut c_void;

#[cfg(windows)]
mod sys {
    use std::ffi::{c_char, c_void};

    use super::CreateInterfaceFn;

    unsafe extern "system" {
        fn GetModuleHandleA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    pub fn create_interface() -> Option<CreateInterfaceFn> {
        // SAFETY: a lookup of an already loaded module by a null terminated name.
        let module = unsafe { GetModuleHandleA(c"vphysics.dll".as_ptr()) };
        if module.is_null() {
            return None;
        }
        // SAFETY: `module` is a loaded module's handle, the name ends in a null.
        let p = unsafe { GetProcAddress(module, c"CreateInterface".as_ptr()) };
        // SAFETY: Source's interface factory, whose C signature the type declares.
        (!p.is_null()).then(|| unsafe { std::mem::transmute::<*mut c_void, CreateInterfaceFn>(p) })
    }
}

#[cfg(not(windows))]
mod sys {
    use super::CreateInterfaceFn;

    pub fn create_interface() -> Option<CreateInterfaceFn> {
        None
    }
}
