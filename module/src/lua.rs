//! The bit of LuaJIT's C API the module uses, taken from the `lua_shared` GMod has already loaded.
//!
//! The `lua_State *` GMod hands to `gmod13_open` is LuaJIT's own, so the plain C API works on it.

use std::ffi::{CStr, c_char, c_double, c_int, c_void};
use std::sync::OnceLock;

use crate::ffi::vphysics_nm::IPhysicsObject;

#[repr(C)]
pub struct LuaState {
    _private: [u8; 0],
}

/// A native function. `C-unwind`: [`raise`] unwinds out of it with `lua_error`.
pub type CFunction = unsafe extern "C-unwind" fn(*mut LuaState) -> c_int;

pub const GLOBALSINDEX: c_int = -10002;

pub const TNIL: c_int = 0;
pub const TBOOLEAN: c_int = 1;
pub const TNUMBER: c_int = 3;
pub const TSTRING: c_int = 4;
pub const TTABLE: c_int = 5;
pub const TUSERDATA: c_int = 7;

/// GMod's `GarrysMod::Lua::Type` values of its userdata.
pub mod gmod_type {
    pub const VECTOR: u8 = 10;
    pub const PHYSOBJ: u8 = 12;
}

/// The block every GMod userdata (`Vector`, `Entity`, `PhysObj`...) points at.
#[repr(C)]
struct UserData {
    data: *mut c_void,
    kind: u8,
}

pub struct Api {
    pub settop: unsafe extern "C" fn(*mut LuaState, c_int),
    pub type_: unsafe extern "C" fn(*mut LuaState, c_int) -> c_int,
    pub tonumber: unsafe extern "C" fn(*mut LuaState, c_int) -> c_double,
    pub toboolean: unsafe extern "C" fn(*mut LuaState, c_int) -> c_int,
    pub touserdata: unsafe extern "C" fn(*mut LuaState, c_int) -> *mut c_void,
    pub tolstring: unsafe extern "C" fn(*mut LuaState, c_int, *mut usize) -> *const c_char,
    pub getfield: unsafe extern "C" fn(*mut LuaState, c_int, *const c_char),
    pub setfield: unsafe extern "C" fn(*mut LuaState, c_int, *const c_char),
    pub rawgeti: unsafe extern "C" fn(*mut LuaState, c_int, c_int),
    pub rawseti: unsafe extern "C" fn(*mut LuaState, c_int, c_int),
    pub createtable: unsafe extern "C" fn(*mut LuaState, c_int, c_int),
    pub pushnil: unsafe extern "C" fn(*mut LuaState),
    pub pushnumber: unsafe extern "C" fn(*mut LuaState, c_double),
    pub pushboolean: unsafe extern "C" fn(*mut LuaState, c_int),
    pub pushstring: unsafe extern "C" fn(*mut LuaState, *const c_char),
    pub pushlstring: unsafe extern "C" fn(*mut LuaState, *const c_char, usize),
    pub pushcclosure: unsafe extern "C" fn(*mut LuaState, CFunction, c_int),
    pub pcall: unsafe extern "C" fn(*mut LuaState, c_int, c_int, c_int) -> c_int,
    pub error: unsafe extern "C-unwind" fn(*mut LuaState) -> c_int,
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

/// The API, loaded on first use. Fails if `lua_shared` isn't in the process or lacks a function.
pub fn api() -> Result<&'static Api, &'static str> {
    API.get_or_init(load).as_ref().map_err(String::as_str)
}

fn load() -> Result<Api, String> {
    let lib = sys::open()?;
    macro_rules! get {
        ($name:literal) => {{
            let p = sys::symbol(lib, concat!($name, "\0"));
            if p.is_null() {
                return Err(format!("lua_shared has no {}", $name));
            }
            // SAFETY: the exported LuaJIT function of that name, whose C signature the field declares.
            unsafe { std::mem::transmute::<*mut c_void, _>(p) }
        }};
    }
    Ok(Api {
        settop: get!("lua_settop"),
        type_: get!("lua_type"),
        tonumber: get!("lua_tonumber"),
        toboolean: get!("lua_toboolean"),
        touserdata: get!("lua_touserdata"),
        tolstring: get!("lua_tolstring"),
        getfield: get!("lua_getfield"),
        setfield: get!("lua_setfield"),
        rawgeti: get!("lua_rawgeti"),
        rawseti: get!("lua_rawseti"),
        createtable: get!("lua_createtable"),
        pushnil: get!("lua_pushnil"),
        pushnumber: get!("lua_pushnumber"),
        pushboolean: get!("lua_pushboolean"),
        pushstring: get!("lua_pushstring"),
        pushlstring: get!("lua_pushlstring"),
        pushcclosure: get!("lua_pushcclosure"),
        pcall: get!("lua_pcall"),
        error: get!("lua_error"),
    })
}

/// A native's body: the number of results, or an error raised in Lua as a string.
pub type Native = unsafe fn(&Api, *mut LuaState) -> Result<c_int, String>;

/// Runs `body`, turning its `Err` into a Lua error.
///
/// # Safety
/// `l` is a live Lua state, called from its own thread inside a native call, so `lua_error` has a frame to
/// unwind to.
pub unsafe fn run(l: *mut LuaState, body: Native) -> c_int {
    let Ok(api) = api() else { return 0 };
    let message = match unsafe { body(api, l) } {
        Ok(n) => return n,
        Err(message) => message,
    };
    unsafe { (api.pushlstring)(l, message.as_ptr().cast(), message.len()) };
    // Nothing that needs dropping may be alive while lua_error unwinds.
    drop(message);
    unsafe { (api.error)(l) }
}

/// Defines `extern "C-unwind" fn $name` running the [`Native`] `$body`.
macro_rules! native {
    ($name:ident => $body:path) => {
        unsafe extern "C-unwind" fn $name(l: *mut $crate::lua::LuaState) -> std::ffi::c_int {
            // SAFETY: called by Lua as a native function.
            unsafe { $crate::lua::run(l, $body) }
        }
    };
}
pub(crate) use native;

impl Api {
    /// Calls the global `print` with `text`, as GMod's console shows it. Errors from it are dropped.
    ///
    /// # Safety
    /// `l` is a live Lua state, called from its own thread.
    pub unsafe fn print(&self, l: *mut LuaState, text: &CStr) {
        unsafe {
            (self.getfield)(l, GLOBALSINDEX, c"print".as_ptr());
            (self.pushstring)(l, text.as_ptr());
            if (self.pcall)(l, 1, 0, 0) != 0 {
                (self.settop)(l, -2);
            }
        }
    }

    /// # Safety
    /// `l` is a live Lua state, called from its own thread.
    pub unsafe fn push_str(&self, l: *mut LuaState, s: &str) {
        unsafe { (self.pushlstring)(l, s.as_ptr().cast(), s.len()) };
    }

    /// Sets `t[name] = n` on the table at the top of the stack.
    ///
    /// # Safety
    /// `l` is a live Lua state with a table on top.
    pub unsafe fn set_number(&self, l: *mut LuaState, name: &CStr, n: f64) {
        unsafe {
            (self.pushnumber)(l, n);
            (self.setfield)(l, -2, name.as_ptr());
        }
    }

    /// Sets `t[name] = s` on the table at the top of the stack.
    ///
    /// # Safety
    /// `l` is a live Lua state with a table on top.
    pub unsafe fn set_str(&self, l: *mut LuaState, name: &CStr, s: &str) {
        unsafe {
            self.push_str(l, s);
            (self.setfield)(l, -2, name.as_ptr());
        }
    }

    /// Sets `t[name] = b` on the table at the top of the stack.
    ///
    /// # Safety
    /// `l` is a live Lua state with a table on top.
    pub unsafe fn set_bool(&self, l: *mut LuaState, name: &CStr, b: bool) {
        unsafe {
            (self.pushboolean)(l, b.into());
            (self.setfield)(l, -2, name.as_ptr());
        }
    }

    /// Sets `t[name] = {x, y, z[, w]}` on the table at the top of the stack.
    ///
    /// # Safety
    /// `l` is a live Lua state with a table on top.
    pub unsafe fn set_vector(&self, l: *mut LuaState, name: &CStr, v: &[f32]) {
        unsafe {
            self.push_vector(l, v);
            (self.setfield)(l, -2, name.as_ptr());
        }
    }

    /// Pops the value on top into `t[i]` of the table under it.
    ///
    /// # Safety
    /// `l` is a live Lua state with a table under the value on top.
    pub unsafe fn append(&self, l: *mut LuaState, i: usize) {
        unsafe { (self.rawseti)(l, -2, i as c_int) };
    }

    /// The string at `arg`.
    ///
    /// # Safety
    /// `l` is a live Lua state, called from its own thread.
    pub unsafe fn check_string(&self, l: *mut LuaState, arg: c_int) -> Result<String, String> {
        if unsafe { (self.type_)(l, arg) } != TSTRING {
            return Err(format!("bad argument #{arg} (string expected)"));
        }
        let mut len = 0;
        let p = unsafe { (self.tolstring)(l, arg, &mut len) };
        // SAFETY: Lua's string of `len` bytes, alive while it's on the stack.
        Ok(String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len) }).into_owned())
    }

    /// The bytes of the string at `arg`, as they are (a file's contents).
    ///
    /// # Safety
    /// `l` is a live Lua state, called from its own thread; the slice lives while the string is on the stack.
    pub unsafe fn check_bytes<'a>(&self, l: *mut LuaState, arg: c_int) -> Result<&'a [u8], String> {
        if unsafe { (self.type_)(l, arg) } != TSTRING {
            return Err(format!("bad argument #{arg} (string expected)"));
        }
        let mut len = 0;
        let p = unsafe { (self.tolstring)(l, arg, &mut len) };
        // SAFETY: Lua's string of `len` bytes, alive while it's on the stack.
        Ok(unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len) })
    }

    /// Pushes `{x =, y =, z = [, w =]}` from 3 or 4 components.
    ///
    /// # Safety
    /// `l` is a live Lua state, called from its own thread.
    pub unsafe fn push_vector(&self, l: *mut LuaState, v: &[f32]) {
        const NAMES: [&CStr; 4] = [c"x", c"y", c"z", c"w"];
        unsafe {
            (self.createtable)(l, 0, v.len() as c_int);
            for (name, &c) in NAMES.iter().zip(v) {
                self.set_number(l, name, f64::from(c));
            }
        }
    }

    /// The `PhysObj` argument at `arg`, as the `IPhysicsObject *` it wraps.
    ///
    /// # Safety
    /// `l` is a live Lua state, called from its own thread.
    pub unsafe fn check_physobj(&self, l: *mut LuaState, arg: c_int) -> Result<*mut IPhysicsObject, String> {
        let data = unsafe { self.check_userdata(l, arg, gmod_type::PHYSOBJ) }
            .ok_or_else(|| format!("bad argument #{arg} (PhysObj expected)"))?;
        if data.is_null() {
            return Err(format!("bad argument #{arg} (invalid PhysObj)"));
        }
        Ok(data.cast())
    }

    /// The data pointer of a GMod userdata of type `kind` at `idx`, `None` for anything else.
    unsafe fn check_userdata(&self, l: *mut LuaState, idx: c_int, kind: u8) -> Option<*mut c_void> {
        if unsafe { (self.type_)(l, idx) } != TUSERDATA {
            return None;
        }
        let ud = unsafe { (self.touserdata)(l, idx) }.cast::<UserData>();
        // SAFETY: every userdata GMod hands to Lua starts with its `UserData` block.
        (!ud.is_null() && unsafe { (*ud).kind } == kind).then(|| unsafe { (*ud).data })
    }

    /// # Safety
    /// `l` is a live Lua state, called from its own thread.
    pub unsafe fn check_number(&self, l: *mut LuaState, arg: c_int) -> Result<f64, String> {
        if unsafe { (self.type_)(l, arg) } != TNUMBER {
            return Err(format!("bad argument #{arg} (number expected)"));
        }
        Ok(unsafe { (self.tonumber)(l, arg) })
    }

    /// The number at `arg`, `default` for none or nil.
    ///
    /// # Safety
    /// `l` is a live Lua state, called from its own thread.
    pub unsafe fn opt_number(&self, l: *mut LuaState, arg: c_int, default: f64) -> Result<f64, String> {
        if unsafe { (self.type_)(l, arg) } <= TNIL {
            return Ok(default);
        }
        unsafe { self.check_number(l, arg) }
    }

    /// A vector argument: a `Vector`, a table `{x =, y =, z =}` or `{x, y, z}`.
    ///
    /// # Safety
    /// `l` is a live Lua state, called from its own thread.
    pub unsafe fn check_vec3(&self, l: *mut LuaState, arg: c_int) -> Result<[f32; 3], String> {
        let kind = unsafe { (self.type_)(l, arg) };
        let is_vector = unsafe { self.check_userdata(l, arg, gmod_type::VECTOR) }.is_some();
        if kind != TTABLE && !is_vector {
            return Err(format!("bad argument #{arg} (Vector or {{x, y, z}} expected)"));
        }
        let mut v = [0.0; 3];
        for (i, (c, name)) in v.iter_mut().zip([c"x", c"y", c"z"]).enumerate() {
            unsafe {
                (self.getfield)(l, arg, name.as_ptr());
                if kind == TTABLE && (self.type_)(l, -1) == TNIL {
                    (self.settop)(l, -2);
                    (self.rawgeti)(l, arg, i as c_int + 1);
                }
                let ok = (self.type_)(l, -1) == TNUMBER;
                *c = (self.tonumber)(l, -1) as f32;
                (self.settop)(l, -2);
                if !ok {
                    return Err(format!("bad argument #{arg} (component {} isn't a number)", i + 1));
                }
            }
        }
        Ok(v)
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::{c_char, c_void};

    unsafe extern "system" {
        fn GetModuleHandleA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    pub fn open() -> Result<*mut c_void, String> {
        // SAFETY: a lookup of an already loaded module by a null terminated name.
        let module = unsafe { GetModuleHandleA(c"lua_shared.dll".as_ptr()) };
        if module.is_null() { Err("lua_shared.dll isn't loaded".into()) } else { Ok(module) }
    }

    pub fn symbol(module: *mut c_void, name: &str) -> *mut c_void {
        // SAFETY: `module` is a loaded module's handle, `name` ends in a null.
        unsafe { GetProcAddress(module, name.as_ptr().cast()) }
    }
}

// Linux (lua_shared.so / lua_shared_srv.so) comes with the Linux builds (ROADMAP M8).
#[cfg(not(windows))]
mod sys {
    use std::ffi::c_void;

    pub fn open() -> Result<*mut c_void, String> {
        Err("gmsv_dms runs on Windows only for now".into())
    }

    pub fn symbol(_module: *mut c_void, _name: &str) -> *mut c_void {
        std::ptr::null_mut()
    }
}
