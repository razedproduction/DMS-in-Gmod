//! VPhysics-Box3D's `IPhysicsNM` (`VBOX3D_NM_002`), as `include/vphysics_nm.h` of the fork declares it.
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

pub const INTERFACE_VERSION: &str = "VBOX3D_NM_002";
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
    /// Only in `VBOX3D_NM_002`: an `_001` vtable ends before it.
    pub set_joint_limits: unsafe extern "C" fn(
        this: *mut IPhysicsNM,
        constraint: *mut IPhysicsConstraint,
        parent: *mut IPhysicsObject,
        world_frames: *const f32,
        hinge: bool,
        limits: *const f32,
    ) -> bool,
}

/// Which version of the interface the loaded vphysics gave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    V001 = 1,
    V002 = 2,
}

impl Version {
    pub fn name(self) -> &'static str {
        match self {
            Version::V001 => INTERFACE_VERSION_001,
            Version::V002 => INTERFACE_VERSION,
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
    for (version, name) in [(Version::V002, c"VBOX3D_NM_002"), (Version::V001, c"VBOX3D_NM_001")] {
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
    #[allow(dead_code)] // Bound in M3.
    pub unsafe fn set_joint_anchor(&self, constraint: *mut IPhysicsConstraint, world_point: Option<&[f32; 3]>) -> bool {
        let point = world_point.map_or(ptr::null(), |p| p.as_ptr());
        unsafe { (self.vtbl().set_joint_anchor)(self.this, constraint, point) }
    }

    /// `None` on `VBOX3D_NM_001`, whose vtable has no such slot.
    ///
    /// # Safety
    /// `constraint` and `parent` are null or live objects of this vphysics.
    #[allow(dead_code)] // Bound in M3.
    pub unsafe fn set_joint_limits(
        &self,
        constraint: *mut IPhysicsConstraint,
        parent: *mut IPhysicsObject,
        world_frames: &[[f32; 4]; 2],
        hinge: bool,
        limits: &[f32; 3],
    ) -> Option<bool> {
        if self.version != Version::V002 {
            return None;
        }
        let frames = world_frames.as_ptr().cast::<f32>();
        Some(unsafe { (self.vtbl().set_joint_limits)(self.this, constraint, parent, frames, hinge, limits.as_ptr()) })
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
