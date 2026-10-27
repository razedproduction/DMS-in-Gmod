//! The Delirium release the module is built against, for `DMS.DeliriumVersion`: the SDK checkout's tag (`v0.2.2`), as
//! its crates all say 0.1.0 and `nm_version` 100. Only read: `git describe` writes nothing into the SDK's folder.

use std::process::Command;

const SDK: &str = "../../delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)";

fn main() {
    let release = Command::new("git")
        .args(["-C", SDK, "describe", "--tags", "--always"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=DMS_DELIRIUM_RELEASE={release}");
    // A checkout, pull or new tag moves HEAD's reflog.
    println!("cargo:rerun-if-changed={SDK}/.git/logs/HEAD");
    println!("cargo:rerun-if-changed={SDK}/.git/packed-refs");
}
