//! gmsv_dms: Delirium active ragdolls (FOLLOW) on VPhysics-Box3D's `IPhysicsNM`.
//!
//! `require("dms")` registers the global table `DMS`.

mod agents;
mod bench;
mod bind;
mod data;
mod dev;
mod ffi;
mod follow;
mod lua;
mod natives;
mod nm;
mod nm_natives;
mod selftest;
mod step;
mod world;

use std::ffi::{CString, c_int};

use ffi::vphysics_nm::{self, AcquireError, Version};
use lua::{Api, LuaState};

const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

// Only the x86-64 branch of GMod is built for.
const ARCH: &str = "win64";

/// The SDK's version (`major * 10000 + minor * 100 + patch`) as text.
fn sdk_version() -> String {
    let v = delirium_physics::nm_version();
    format!("{}.{}.{}", v / 10000, v / 100 % 100, v % 100)
}

lua::native!(lua_nm_version => nm_version_impl);
/// `DMS.NmVersion()`: the Delirium SDK's version number, as `nm_version` gives it.
unsafe fn nm_version_impl(api: &Api, l: *mut LuaState) -> Result<c_int, String> {
    unsafe { (api.pushnumber)(l, f64::from(delirium_physics::nm_version())) };
    Ok(1)
}

/// Looks for `IPhysicsNM` and says in the console what came of it (docs/adr/0005).
fn acquire_message() -> String {
    match vphysics_nm::acquire() {
        Ok(Version::V003) => format!("[DMS] IPhysicsNM {} found", Version::V003.name()),
        Ok(Version::V002) => format!(
            "[DMS] WARNING: only IPhysicsNM {} found, Homer's full inertia and NM's ball joints (SetBodyInertia, \
             SetJointBallLimits) are unavailable: update VPhysics-Box3D",
            Version::V002.name()
        ),
        Ok(Version::V001) => format!(
            "[DMS] WARNING: only IPhysicsNM {} found, joint limits (SetJointLimits) are unavailable: \
             update VPhysics-Box3D",
            Version::V001.name()
        ),
        Err(AcquireError::NoVphysics) => {
            "[DMS] ERROR: vphysics.dll isn't loaded, DMS is disabled".into()
        }
        Err(AcquireError::NotBox3d) => "[DMS] ERROR: the installed vphysics.dll isn't VPhysics-Box3D (no IPhysicsNM), \
                                        DMS is disabled: reinstall VPhysics-Box3D into garrysmod/bin/win64 (Steam's \
                                        file verification puts the original back)"
            .into(),
    }
}

/// # Safety
/// Called by GMod with its live server Lua state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmod13_open(l: *mut LuaState) -> c_int {
    // Without lua_shared there's no way to report anything: the module just stays inert.
    let Ok(api) = lua::api() else { return 0 };
    let nm_message = acquire_message();
    unsafe {
        let natives = natives::NATIVES.iter().chain(nm_natives::NATIVES).chain(bind::NATIVES).chain(agents::NATIVES).chain(dev::NATIVES).chain(world::NATIVES);
        (api.createtable)(l, 0, 2 + natives.clone().count() as c_int);
        let version = CString::new(MODULE_VERSION).unwrap_or_default();
        (api.pushstring)(l, version.as_ptr());
        (api.setfield)(l, -2, c"Version".as_ptr());
        (api.pushcclosure)(l, lua_nm_version, 0);
        (api.setfield)(l, -2, c"NmVersion".as_ptr());
        for (name, f) in natives {
            (api.pushcclosure)(l, *f, 0);
            (api.setfield)(l, -2, name.as_ptr());
        }
        (api.setfield)(l, lua::GLOBALSINDEX, c"DMS".as_ptr());

        let banner = format!("[DMS] gmsv_dms {MODULE_VERSION} ({ARCH}), Delirium SDK {}", sdk_version());
        api.print(l, &CString::new(banner).unwrap_or_default());
        api.print(l, &CString::new(nm_message).unwrap_or_default());
    }
    0
}

/// # Safety
/// Called by GMod with the Lua state given to `gmod13_open`, as it shuts it down.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmod13_close(l: *mut LuaState) -> c_int {
    dev::stop();
    step::shutdown();
    vphysics_nm::release();
    if let Ok(api) = lua::api() {
        unsafe {
            (api.pushnil)(l);
            (api.setfield)(l, lua::GLOBALSINDEX, c"DMS".as_ptr());
        }
    }
    0
}
