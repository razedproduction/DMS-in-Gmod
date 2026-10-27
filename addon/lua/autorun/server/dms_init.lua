-- The clients' part (ROADMAP M6): Homer's face, skeleton and debug overlay, the DMS window.
AddCSLuaFile("autorun/client/dms_client.lua")
AddCSLuaFile("dms/cl_face.lua")
AddCSLuaFile("dms/cl_overlay.lua")
AddCSLuaFile("dms/cl_menu.lua")

-- This addon's release: the module has to be the same one (scripts\package.ps1 checks both are Cargo.toml's).
DMS_ADDON_VERSION = "0.1.0"
-- The IPhysicsNM the module binds Homer with (docs/adr/0018).
local INTERFACE = "VBOX3D_NM_004"

-- What's wrong, to superadmins as they join (the console has it already).
local function tellSuperadmins(name, lines)
	hook.Add("PlayerInitialSpawn", name, function(ply)
		if not ply:IsSuperAdmin() then return end
		for _, l in ipairs(lines) do ply:ChatPrint(l) end
		ply:ChatPrint("[DMS] How to fix it: docs/install.md (FAQ)")
	end)
end

if not util.IsBinaryModuleInstalled("dms") then
	local message = "[DMS] gmsv_dms isn't installed: put gmsv_dms_win64.dll into garrysmod/lua/bin (x86-64 branch only)"
	ErrorNoHalt(message .. "\n")
	tellSuperadmins("DMS_NoModule", { message })
	return
end

-- Once a Lua state: this file runs again on lua refresh, with the module and its agents kept (docs/adr/0021).
if not DMS then require("dms") end

-- The module and this Lua from different releases: their natives and tables needn't match, so nothing runs.
if DMS.Version ~= DMS_ADDON_VERSION then
	local message = string.format("[DMS] gmsv_dms_win64.dll is %s but the Lua addon is %s: DMS is disabled, install "
		.. "both from one release (DMS-<version>-win64.zip, install.ps1)", tostring(DMS.Version), DMS_ADDON_VERSION)
	ErrorNoHalt(message .. "\n")
	tellSuperadmins("DMS_Version", { message })
	return
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
include("dms/follow.lua")
include("dms/world.lua")
include("dms/reactions.lua")
include("dms/api.lua")
include("dms/lifecycle.lua")
include("dms/tuning.lua")
include("dms/overlay.lua")
include("dms/bench_scene.lua")
include("dms/dev.lua")
include("dms/spike_ragdolls.lua")

-- What keeps Homer from working here, a line each (empty: nothing): the fork, Homer's model, NM's data. For the
-- console at start, superadmins as they join, and the DMS window's Debug tab.
function DMS_Problems()
	local out = {}
	local iface = DMS.InterfaceVersion()
	if not iface then
		-- docs/adr/0005: without IPhysicsNM nothing runs.
		out[#out + 1] = "vphysics.dll isn't VPhysics-Box3D (no IPhysicsNM): DMS is disabled. Steam's file check puts "
			.. "the original back: reinstall the fork (install.ps1)"
	elseif iface ~= INTERFACE then
		out[#out + 1] = string.format("vphysics.dll is an older VPhysics-Box3D (%s, DMS needs %s): reinstall the fork "
			.. "from this release (install.ps1)", iface, INTERFACE)
	end
	if not util.IsValidModel(DMS_HOMER_MODEL) then
		out[#out + 1] = DMS_HOMER_MODEL .. " isn't there: make it with dms-homer from your GTA IV (install.ps1 -Gta)"
	else
		local made = DMS_HomerModelVersion()
		if made ~= DMS.HomerModelVersion then
			out[#out + 1] = string.format("%s was made by an older dms-homer (model %s, this release needs %d): make it "
				.. "again (install.ps1 -Gta)", DMS_HOMER_MODEL, made and tostring(made) or "unmarked", DMS.HomerModelVersion)
		end
	end
	if DMS_GameDir() == "" then
		out[#out + 1] = "No GTA IV folder for NM's data: dms_game_dir \"<your GTA IV folder>\" (or install.ps1 -Gta)"
	end
	return out
end

do
	local problems = DMS_Problems()
	if #problems > 0 then
		for i, p in ipairs(problems) do
			problems[i] = "[DMS] " .. p
			MsgC(Color(255, 120, 90), problems[i] .. "\n")
		end
		tellSuperadmins("DMS_Problems", problems)
	end
end
