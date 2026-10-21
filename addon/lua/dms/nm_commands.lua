-- Delirium inside the module (ROADMAP M2): NM's data from the user's GTA IV, the self-test, reference lists.
-- Superadmins only. None of it needs IPhysicsNM: NM runs on its own body here.

local gameDir = CreateConVar("dms_game_dir", "", FCVAR_ARCHIVE + FCVAR_PROTECTED,
	"Your GTA IV folder (PC: with GTAIV.exe; Xbox 360: with default.xex): NM's data is read from there")

local function say(ply, text)
	if IsValid(ply) then
		ply:PrintMessage(HUD_PRINTCONSOLE, text)
	else
		print(text)
	end
end

local function allowed(ply)
	return not IsValid(ply) or ply:IsSuperAdmin()
end

-- Loads NM's data from dms_game_dir (once), or says what's missing and where to point it.
local function load(ply)
	local ok, info = pcall(DMS.LoadData, gameDir:GetString())
	if not ok then
		say(ply, "[DMS] NM's data isn't loaded: " .. tostring(info))
		return
	end
	return info
end

local function vec(v)
	if not v then return "-" end
	if v.w then return string.format("(%.4f, %.4f, %.4f, %.4f)", v.x, v.y, v.z, v.w) end
	return string.format("(%.4f, %.4f, %.4f)", v.x, v.y, v.z)
end

concommand.Add("dms_data", function(ply)
	if not allowed(ply) then return end
	local d = load(ply)
	if not d then return end
	say(ply, string.format("[DMS] NM's data: %s (%s), Homer %d parts, %d surfaces (floor row %d), taskparams: %s",
		d.dir, d.platform, d.parts, d.surfaces, d.floorMaterial, d.taskparams))
end)

-- Homer in LEAD on a flat floor under bodyBalance for [seconds = 2]: he has to stay up.
concommand.Add("dms_selftest", function(ply, _, args)
	if not allowed(ply) then return end
	if not load(ply) then return end
	local ok, r = pcall(DMS.SelfTest, tonumber(args[1]) or 2)
	if not ok then return say(ply, "[DMS] selftest: " .. tostring(r)) end
	say(ply, string.format("[DMS] selftest: %s", r.verdict))
	say(ply, string.format("  %d steps of 1/60 s; pelvis %.3f -> min %.3f -> %.3f m; head %.3f -> %.3f m; " ..
		"balancing %s, fallen %s", r.steps, r.pelvisStart, r.pelvisMin, r.pelvisEnd, r.headStart, r.headEnd,
		tostring(r.balancing), tostring(r.fallen)))
end)

concommand.Add("dms_nm_parts", function(ply)
	if not allowed(ply) then return end
	for i, name in ipairs(DMS.PartNames()) do
		say(ply, string.format("  %2d %s", i - 1, name))
	end
end)

-- Every message, or [message]'s params.
concommand.Add("dms_nm_messages", function(ply, _, args)
	if not allowed(ply) then return end
	if not args[1] then
		local names = DMS.Messages()
		say(ply, string.format("[DMS] %d messages (dms_nm_messages <name> for its params):", #names))
		say(ply, "  " .. table.concat(names, " "))
		return
	end
	local ok, params = pcall(DMS.MessageParams, args[1])
	if not ok then return say(ply, "[DMS] " .. tostring(params)) end
	say(ply, string.format("[DMS] %s: %d params (* kept only, the port doesn't act on it)", args[1], #params))
	for _, p in ipairs(params) do
		local default = type(p.default) == "table" and vec(p.default) or tostring(p.default)
		say(ply, string.format("  %s%s %s = %s  %s", p.name, p.used and "" or "*", p.kind, default, p.doc))
	end
end)

concommand.Add("dms_nm_build", function(ply)
	if not allowed(ply) then return end
	if not load(ply) then return end
	for _, p in ipairs(DMS.BuildParts()) do
		say(ply, string.format("  %2d %-16s parent %2d  %.3f kg  r %.4f  half %.4f  pos %s  rot %s", p.index, p.name,
			p.parent, p.mass, p.radius, p.half, vec(p.pos), vec(p.rot)))
	end
end)

concommand.Add("dms_nm_joints", function(ply)
	if not allowed(ply) then return end
	if not load(ply) then return end
	local names = DMS.PartNames()
	for _, j in ipairs(DMS.BuildJoints()) do
		local limits = j.kind == "hinge" and string.format("[%.4f, %.4f]", j.limits.x, j.limits.y) or vec(j.limits)
		say(ply, string.format("  %-16s <- %-16s %-5s pivot %s  frame %s  limits %s  armature %g", names[j.part + 1],
			names[j.parent + 1], j.kind, vec(j.pivot), vec(j.frame), limits, j.armature))
	end
end)
