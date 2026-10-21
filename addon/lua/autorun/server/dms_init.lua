if not util.IsBinaryModuleInstalled("dms") then
	ErrorNoHalt("[DMS] gmsv_dms isn't installed: put gmsv_dms_win64.dll into garrysmod/lua/bin (x86-64 branch only)\n")
	return
end

require("dms")

-- docs/adr/0005: without IPhysicsNM nothing runs, superadmins are told why.
if not DMS.Available() then
	local message = "[DMS] vphysics.dll isn't VPhysics-Box3D (no IPhysicsNM): DMS is disabled, reinstall VPhysics-Box3D"
	hook.Add("PlayerInitialSpawn", "DMS_NoPhysicsNM", function(ply)
		if ply:IsSuperAdmin() then ply:ChatPrint(message) end
	end)
end

-- The step callback goes before the physics environment does.
hook.Add("ShutDown", "DMS_Shutdown", function()
	DMS.Shutdown()
end)

include("dms/debug_commands.lua")
include("dms/nm_commands.lua")
include("dms/bench_scene.lua")
include("dms/dev.lua")
