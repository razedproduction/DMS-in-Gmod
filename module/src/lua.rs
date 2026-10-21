//! The bit of LuaJIT's C API the module uses, taken from the `lua_shared` GMod has already loaded.
//!
//! The `lua_State *` GMod hands to `gmod13_open` is LuaJIT's own, so the plain C API works on it.

use std::ffi::{CStr, c_char, c_double, c_int, c_void};
use std::sync::OnceLock;

#[repr(C)]
pub struct LuaState {
    _private: [u8; 0],
}

pub type CFunction = unsafe extern "C" fn(*mut LuaState) -> c_int;

pub const GLOBALSINDEX: c_int = -10002;

pub struct Api {
    pub settop: unsafe extern "C" fn(*mut LuaState, c_int),
    pub getfield: unsafe extern "C" fn(*mut LuaState, c_int, *const c_char),
    pub setfield: unsafe extern "C" fn(*mut LuaState, c_int, *const c_char),
    pub createtable: unsafe extern "C" fn(*mut LuaState, c_int, c_int),
    pub pushnil: unsafe extern "C" fn(*mut LuaState),
    pub pushnumber: unsafe extern "C" fn(*mut LuaState, c_double),
    pub pushstring: unsafe extern "C" fn(*mut LuaState, *const c_char),
    pub pushcclosure: unsafe extern "C" fn(*mut LuaState, CFunction, c_int),
    pub pcall: unsafe extern "C" fn(*mut LuaState, c_int, c_int, c_int) -> c_int,
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
        getfield: get!("lua_getfield"),
        setfield: get!("lua_setfield"),
        createtable: get!("lua_createtable"),
        pushnil: get!("lua_pushnil"),
        pushnumber: get!("lua_pushnumber"),
        pushstring: get!("lua_pushstring"),
        pushcclosure: get!("lua_pushcclosure"),
        pcall: get!("lua_pcall"),
    })
}

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
