# VPhysics Box3D

This is a reimplementation of Source VPhysics (IVP/Havok) on [Box3D](https://github.com/erincatto/box3d).

For discussions, please join the Discord server [here](https://discord.gg/rbCJdZjewf).

## Features

| Feature | VPhysics | Volt | VBox3D |
|:--|:--:|:--:|:--:|
| Constraints (except pulleys) | ✔️ | ✔️ | ✔️ |
| Pulleys | ✔️ | ✔️ | ✔️ |
| Breakable constraints | ✔️ | ❌ | ✔️ |
| Motion controllers | ✔️ | ✔️ | ✔️ |
| Constraint motors | ✔️ | ✔️ | ✔️ |
| Ragdolls | ✔️ | ✔️ | ✔️ |
| Triggers | ✔️ | ✔️ | ✔️ |
| Object touch callbacks | ✔️ | ✔️ | ✔️ |
| Prop damage / breaking | ✔️ | ✔️ | ✔️ |
| Fluid events | ✔️ | ✔️ | ✔️ |
| Prop splashing effects | ✔️ | ✔️ | ✔️ |
| Wheeled vehicles | ✔️ | ✔️ | ✔️ |
| Raycast vehicles (airboat) | ✔️ | ❌ | ✔️ |
| Shadow controllers (NPCs, doors) | ✔️ | ✔️ | ✔️ |
| Save / restore | ✔️ | ✔️ | ✔️ |
| Per-object no-collide callbacks | ✔️ | ✔️ | ✔️ |
| Crash-resistant solver | ❌ | ✔️ | ✔️ |
| Thousands of objects without lag | ❌ | ✔️ | ✔️ |
| Multithreaded | ❌ | ✔️ | ✔️ |
| Player controller | ✔️ | ✔️ | ✔️ |

## Platforms

One build works for both Garry's Mod and Source SDK 2013 Multiplayer games, just pick the preset for your platform.

| Preset | Builds | Tested |
|:--|:--:|:--:|
| `windows-x64` | ✔️ | ✔️ |
| `windows-x86` | ✔️ | ✔️ |
| `linux-x64` | ✔️ |  |
| `linux-x86` | ✔️ |  |
| `dedicated-linux-x64` | ✔️ |  |
| `dedicated-linux-x86` | ✔️ | ✔️ |

Windows dedicated servers load the same `vphysics.dll` as the game, so they use the Windows presets.

## Building

You'll need:

* **CMake** 3.25 or newer
* **Windows:** Visual Studio 2026 with the **Desktop development with C++** workload, plus **C++ Clang tools for Windows** for `windows-x86`
* **Linux:** Podman or Docker

Clone the repository with its submodules:

```
git clone --recursive https://github.com/Asphaltian/VPhysics-Box3D.git
cd VPhysics-Box3D
```

On Windows, configure and build the preset for your platform:

```
cmake --preset windows-x64
cmake --build --preset windows-x64-release
```

Linux builds are made inside Valve's Steam Runtime SDK, the same environment the games themselves are built in:

```
podman run --rm -v "$PWD":/src -w /src registry.gitlab.steamos.cloud/steamrt/sniper/sdk:latest \
	sh -c "cmake --preset linux-x86 && cmake --build --preset linux-x86-release"
```

The library ends up in `build/<preset>/src/Release/`. Swap `-release` for `-debug` if you need a debug build.

## Installing

Replace the game's own `vphysics` library with the one you built:

| Game | Windows | Linux |
|:--|:--|:--|
| Garry's Mod | `bin/win64/vphysics.dll` (x64), `bin/vphysics.dll` (x86) | `bin/vphysics.so` |
| Garry's Mod Dedicated Server | `bin/win64/vphysics.dll` (x64), `bin/vphysics.dll` (x86) | `bin/vphysics_srv.so` |
| Source SDK Base 2013 Multiplayer | `bin/x64/vphysics.dll` (x64), `bin/vphysics.dll` (x86) | `bin/linux64/vphysics.so` (x64), `bin/vphysics.so` (x86) |
| Source SDK Base 2013 Dedicated Server | `bin/x64/vphysics.dll` (x64), `bin/vphysics.dll` (x86) | `bin/linux64/vphysics_srv.so` (x64), `bin/vphysics_srv.so` (x86) |

Keep in mind that Steam puts the original back whenever it verifies or updates the game files, so back it up and copy yours over again afterwards.

## Active ragdoll extension (`VBOX3D_NM_002`)

VBox3D answers one extra interface, `IPhysicsNM`, for active ragdoll controllers (NaturalMotion style FOLLOW mode: the game's ragdoll is the body, the controller reads its parts every physics step and returns a force and torque per part). The header is [`include/vphysics_nm.h`](include/vphysics_nm.h), it only needs forward declarations of the standard VPhysics interfaces, so copy it into your module. It gives you:

* each body's state: centre of mass position, rotation, linear and angular velocity, in metres, radians and seconds, world axes as Source's (Z up);
* per-body drives: a force and torque (N, Nm, world) applied continuously over all Box3D sub-steps of the next step, unlike `IPhysicsObject::ApplyForceCenter`, which adds one velocity change per step;
* per-joint armature (kg m², as MuJoCo's joint armature): inertia added to the relative rotation of a constraint's two objects, solved inside every Box3D sub-step together with the joint and its limits, so it resists drives, gravity, contacts and other joints alike. NM's `nm_agent_get_joint` returns it per joint each step, and stiff drives need it to stay stable;
* joint pivots: `SetJointAnchor` moves a ragdoll joint's pivot to a world point (taken in both objects' current poses, limit frames keep their orientation), so it pivots where the controller's joint does; a null point puts the constraint's own pivot back;
* joint limits: `SetJointLimits` gives a ragdoll joint the controller joint's frame (a world rotation, taken in both objects' current poses, which become the joint's zero) and limits (a hinge's range about the frame's Z, or a ball's turn either way about its X, Y and Z), so it turns where the controller's joint can and no further. `VBOX3D_NM_001` still answers, the same interface without it;
* a per-environment callback run inside `IPhysicsEnvironment::Simulate` once per physics step (1/66 s by default, 4 sub-steps), right before the step is solved.

Nothing changes when nobody uses the interface.

The armature lives in Box3D's joint solver, which upstream doesn't have. The build copies `external/box3d` into the build tree and applies [`cmake/patches/box3d-joint-armature.patch`](cmake/patches/box3d-joint-armature.patch) there (the submodule stays clean), adding `b3Joint_SetArmature` for every joint type.

### Getting the interface from a binary module

`vphysics` is already loaded by the time a Garry's Mod binary module (`gmsv_*`/`gmcl_*`) is, so ask its `CreateInterface` from `GMOD_MODULE_OPEN` (or later):

```cpp
#include "vphysics_nm.h"

typedef void *( *CreateInterfaceFn )( const char *pName, int *pReturnCode );

static IPhysicsNM *GetPhysicsNM()
{
#ifdef _WIN32
	HMODULE hModule = GetModuleHandleA( "vphysics.dll" );
	CreateInterfaceFn factory = hModule ? reinterpret_cast<CreateInterfaceFn>( GetProcAddress( hModule, "CreateInterface" ) ) : nullptr;
#else
	// vphysics_srv.so on a Linux dedicated server
	void *pModule = dlopen( "vphysics.so", RTLD_LAZY | RTLD_NOLOAD );
	CreateInterfaceFn factory = pModule ? reinterpret_cast<CreateInterfaceFn>( dlsym( pModule, "CreateInterface" ) ) : nullptr;
#endif
	return factory ? static_cast<IPhysicsNM *>( factory( VPHYSICS_NM_INTERFACE_VERSION, nullptr ) ) : nullptr;
}
```

With the Source SDK's tier1 linked in, `Sys_GetFactory( "vphysics" )` does the same, and `IPhysics::QueryInterface( VPHYSICS_NM_INTERFACE_VERSION )` answers it too. A null pointer means the game runs another vphysics: fall back to something else.

`IPhysicsObject *` comes from a Lua `PhysObj` (`LUA->GetUserType<IPhysicsObject>( index, GarrysMod::Lua::Type::PhysObj )`, or `Entity:GetPhysicsObjectNum( i )` for a ragdoll's parts). `FindConstraint( parent, child )` gives the ragdoll joint between two parts and `GetObjectEnvironment( part )` the environment to hook (a listen server runs a client and a server environment in the same process).

### Per step

1. Once per ragdoll: `SetStepCallback( environment, OnStep, context )` (one callback per environment, share it between ragdolls), and, with the ragdoll at rest in its bind pose, `SetJointAnchor` on every joint with the controller's pivot (`nm_agent_get_joint`'s `pivot`), mapped into that pose, then `SetJointLimits` with its frame and limits (`nm_agent_get_joint`'s `frame` and `limits`), mapped the same way.
2. In `OnStep( environment, timestep, context )`, for each driven ragdoll:
   1. `GetBodyState` for every part, convert each to the controller's part frame (a fixed offset per part, taken in the bind pose; the centre of mass is `GetMassCenterLocalSpace()` in the body frame);
   2. step the controller by `timestep` (`nm_agent_set_part_states`, `nm_agent_step`);
   3. `ApplyBodyDrive` for every part with the controller's force and torque (`nm_agent_get_part_drives`);
   4. `SetJointArmature` on every joint with the controller's armature (`nm_agent_get_joint`).
3. The step then runs on those drives. They are cleared after it, armature stays until changed.
4. `SetStepCallback( environment, nullptr, nullptr )` before the context goes away (ragdoll removed, module unloaded), and set the armature back to 0 and the pivots back with `SetJointAnchor( joint, nullptr )` if the ragdoll goes back to being a plain one.

Only read states and set drives and armature from the callback: other game side calls there are queued for the next step or unsafe.

## Media

### Jolt vs. Box3D
[![Jolt vs. Box3D](https://i.ytimg.com/vi/okKg7In3OJw/hqdefault.jpg)](https://www.youtube.com/watch?v=okKg7In3OJw "Jolt vs. Box3D")

### IVP/Havok vs. Jolt vs. Box3D (old)
[![IVP/Havok vs. Jolt vs. Box3D](https://i.ytimg.com/vi/cs8C8T1vg7g/hqdefault.jpg)](https://youtu.be/cs8C8T1vg7g "IVP/Havok vs. Jolt vs. Box3D")

### Watermelons
[![Watermelons](https://i.ytimg.com/vi/yTp4jTYFWJ8/hqdefault.jpg)](https://youtu.be/yTp4jTYFWJ8 "Watermelons")

### Buoyant Mossmans
[![Buoyant Mossmans](https://i.ytimg.com/vi/z4MaxT87Eqs/hqdefault.jpg)](https://youtu.be/z4MaxT87Eqs "Buoyant Mossmans")

### Battle for Flatgrass
[![Battle for Flatgrass](https://i.ytimg.com/vi/BVPiprozkNM/hqdefault.jpg)](https://youtu.be/BVPiprozkNM "Battle for Flatgrass")

### Vehicular Homicide
[![Vehicular Homicide](https://i.ytimg.com/vi/IOlv3qxC7Js/hqdefault.jpg)](https://youtu.be/IOlv3qxC7Js "Vehicular Homicide")

## Credits

* [Box3D](https://github.com/erincatto/box3d) by Erin Catto
* Inspired by [Volt](https://github.com/Joshua-Ashton/VPhysics-Jolt) by Joshua Ashton and Josh Dowell

## License

MIT, see [LICENSE](LICENSE). Box3D, qhull and Source SDK code retain their respective licenses.
