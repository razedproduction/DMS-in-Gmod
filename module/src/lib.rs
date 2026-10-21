//! gmsv_dms: Delirium active ragdolls (FOLLOW) on VPhysics-Box3D's `IPhysicsNM`.
//!
//! `require("dms")` registers the global table `DMS`.

mod lua;

use std::ffi::{CString, c_int};

use lua::LuaState;

const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

// Only the x86-64 branch of GMod is built for.
const ARCH: &str = "win64";

/// The SDK's version (`major * 10000 + minor * 100 + patch`) as text.
fn sdk_version() -> String {
    let v = delirium_physics::nm_version();
    format!("{}.{}.{}", v / 10000, v / 100 % 100, v % 100)
}

/// `DMS.NmVersion()`: the Delirium SDK's version number, as `nm_version` gives it.
unsafe extern "C" fn lua_nm_version(l: *mut LuaState) -> c_int {
    let Ok(api) = lua::api() else { return 0 };
    unsafe { (api.pushnumber)(l, f64::from(delirium_physics::nm_version())) };
    1
}

/// # Safety
/// Called by GMod with its live server Lua state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmod13_open(l: *mut LuaState) -> c_int {
    // Without lua_shared there's no way to report anything: the module just stays inert.
    let Ok(api) = lua::api() else { return 0 };
    unsafe {
        (api.createtable)(l, 0, 2);
        let version = CString::new(MODULE_VERSION).unwrap_or_default();
        (api.pushstring)(l, version.as_ptr());
        (api.setfield)(l, -2, c"Version".as_ptr());
        (api.pushcclosure)(l, lua_nm_version, 0);
        (api.setfield)(l, -2, c"NmVersion".as_ptr());
        (api.setfield)(l, lua::GLOBALSINDEX, c"DMS".as_ptr());

        let banner = format!("[DMS] gmsv_dms {MODULE_VERSION} ({ARCH}), Delirium SDK {}", sdk_version());
        api.print(l, &CString::new(banner).unwrap_or_default());
    }
    0
}

/// # Safety
/// Called by GMod with the Lua state given to `gmod13_open`, as it shuts it down.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmod13_close(l: *mut LuaState) -> c_int {
    if let Ok(api) = lua::api() {
        unsafe {
            (api.pushnil)(l);
            (api.setfield)(l, lua::GLOBALSINDEX, c"DMS".as_ptr());
        }
    }
    0
}
