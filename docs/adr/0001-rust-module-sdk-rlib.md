# 0001. The module in Rust, the Delirium SDK linked as an `rlib` by path

Status: accepted (2026-10-21)

## Context

Delirium is written in Rust. Its SDK (`crates/delirium-sdk`, the `delirium_physics` crate) builds both as a `cdylib`
(the C API `delirium_physics.h`) and as an `rlib`. The folder `delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)`
mustn't be changed. GMod loads binary modules only from `garrysmod/lua/bin` and looks for the DLLs they depend on on
Windows' standard paths, which don't include `lua/bin`.

## Decision

- The binary module `gmsv_dms` is written in Rust (`module/`), a `cdylib` crate.
- The SDK comes in as a path dependency
  `delirium-sdk = { path = "../../delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)/crates/delirium-sdk" }`
  and is called as ordinary Rust functions (`delirium_physics::nm_*`).
- The build goes into this repository's `target/`; the SDK's folder is only read.
- The Lua API (LuaJIT from `lua_shared`) is taken at run time through `GetProcAddress`, without wrapper crates.

## Consequences

- One DLL an architecture, without a separate `delirium_physics.dll` next to the game.
- The module has to build with a Rust version the SDK supports (edition 2024).
- The DLL exports the SDK's `nm_*` symbols too: harmless, but not part of the module's API.
- Updating the SDK is a `git pull` in its folder and a rebuild, without copying code.
