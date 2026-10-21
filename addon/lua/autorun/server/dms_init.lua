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

-- Text to a player's console (the server's for nil): in pieces, as a user message carries 255 bytes at most.
function DMS_Say(ply, text)
	if not IsValid(ply) then
		print(text)
		return
	end
	for line in (text .. "\n"):gmatch("(.-)\n") do
		repeat
			local piece = line:sub(1, 200)
			-- Not in the middle of a UTF-8 character.
			while #piece < #line and #piece > 1 and line:byte(#piece + 1) >= 0x80 and line:byte(#piece + 1) < 0xC0 do
				piece = piece:sub(1, -2)
			end
			ply:PrintMessage(HUD_PRINTCONSOLE, piece)
			line = line:sub(#piece + 1)
		until line == ""
	end
end

include("dms/debug_commands.lua")
include("dms/nm_commands.lua")
include("dms/homer.lua")
include("dms/bench_scene.lua")
include("dms/dev.lua")
