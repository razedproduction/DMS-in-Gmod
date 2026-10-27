-- The player's tuning of NM's behaviours (docs/adr/0009, ROADMAP M6), the server's side: profiles in
-- data/dms/profiles/<name>.json hold only what differs from the Playroom's defaults (messages' params and taskparams'
-- values); "Playroom" is the empty one and can't be changed. dms_tuning 0|1 turns it off/on, dms_tuning_profile picks
-- the profile. Only superadmins change anything; the menu (cl_menu.lua) talks to this through net messages, and
-- changes act from Homers' next NM step. The bench and golden runs never get any (the module skips scenarios).

local DIR = "dms/profiles"
local PLAYROOM = "Playroom"

local tuningOn = CreateConVar("dms_tuning", "0", { FCVAR_ARCHIVE, FCVAR_REPLICATED }, "The player's tuning of NM's behaviours (0 = the Playroom's, docs/adr/0009)")
local profileName = CreateConVar("dms_tuning_profile", PLAYROOM, { FCVAR_ARCHIVE, FCVAR_REPLICATED }, "The tuning profile (data/dms/profiles/<name>.json)")

util.AddNetworkString("dms_tuning")

local function clean(name)
	name = tostring(name or ""):gsub("[^%w_%- ]", ""):sub(1, 48)
	return name ~= "" and name or nil
end

local function path(name) return DIR .. "/" .. name:lower():gsub(" ", "_") .. ".json" end

-- A profile: `{messages = {[msg] = {[param] = "value"}}, taskparams = {["task set param"] = number}}`.
function DMS_TuningLoad(name)
	if name == PLAYROOM then return { messages = {}, taskparams = {} } end
	local t = util.JSONToTable(file.Read(path(name), "DATA") or "")
	if not t then return nil end
	t.messages, t.taskparams = t.messages or {}, t.taskparams or {}
	return t
end

function DMS_TuningList()
	local out = { PLAYROOM }
	for _, f in ipairs(file.Find(DIR .. "/*.json", "DATA")) do
		local t = util.JSONToTable(file.Read(DIR .. "/" .. f, "DATA") or "")
		if t and t.name and t.name ~= PLAYROOM then out[#out + 1] = t.name end
	end
	return out
end

local function save(name, t)
	file.CreateDir(DIR)
	t.name = name
	file.Write(path(name), util.TableToJSON(t, true))
end

-- The active profile to the module (`DMS.SetTuning`): its lines, or off.
function DMS_TuningApply()
	if not DMS.SetTuning or not DMS.DataInfo() then return end
	local name = profileName:GetString()
	local t = tuningOn:GetBool() and DMS_TuningLoad(name)
	if not t then
		pcall(DMS.SetTuning)
		return
	end
	local lines = {}
	for msg, ps in SortedPairs(t.messages) do
		for p, v in SortedPairs(ps) do lines[#lines + 1] = string.format("msg %s %s %s", msg, p, tostring(v)) end
	end
	for key, v in SortedPairs(t.taskparams) do lines[#lines + 1] = string.format("tp %s %s", key, tostring(v)) end
	local ok, err = pcall(DMS.SetTuning, name, table.concat(lines, "\n"))
	if not ok then print("[DMS] tuning " .. name .. ": " .. tostring(err)) end
end
cvars.AddChangeCallback("dms_tuning", DMS_TuningApply, "DMS_Tuning")
cvars.AddChangeCallback("dms_tuning_profile", DMS_TuningApply, "DMS_Tuning")
hook.Add("DMS_Attached", "DMS_Tuning", DMS_TuningApply)

-- What the menu needs: NM's behaviours and their params, the Playroom's reaction table, the profiles.
local function catalogue()
	-- The menu may open before any Homer: NM's data from the game as a spawn loads it (taskparams, weapons need it).
	local why
	if not DMS.DataInfo() then
		local ok, err = pcall(DMS.LoadData, GetConVar("dms_game_dir"):GetString())
		if not ok then why = "NM's data isn't loaded: " .. tostring(err) end
	end
	local msgs = {}
	for _, name in ipairs(DMS.Messages() or {}) do
		local ok, ps = pcall(DMS.MessageParams, name)
		msgs[#msgs + 1] = { name = name, params = ok and ps or {} }
	end
	return {
		messages = msgs,
		reactions = DMS.Reactions(),
		weapons = DMS.Weapons(),
		taskparams = not why and DMS.TaskParams() or {},
		profiles = DMS_TuningList(),
		profile = profileName:GetString(),
		on = tuningOn:GetBool(),
		error = why,
	}
end

local function reply(ply, kind, t)
	local s = util.Compress(util.TableToJSON({ kind = kind, data = t }))
	net.Start("dms_tuning")
	net.WriteUInt(#s, 32)
	net.WriteData(s, #s)
	net.Send(ply)
end

-- The menu's requests: {kind, ...}. Superadmins only.
net.Receive("dms_tuning", function(_, ply)
	if not IsValid(ply) or not ply:IsSuperAdmin() then return end
	local n = net.ReadUInt(32)
	local req = util.JSONToTable(util.Decompress(net.ReadData(n)) or "")
	if not req then return end
	local k = req.kind
	if k == "catalogue" then
		return reply(ply, "catalogue", catalogue())
	elseif k == "profile" then
		local name = clean(req.name) or PLAYROOM
		return reply(ply, "profile", { name = name, profile = DMS_TuningLoad(name) or { messages = {}, taskparams = {} } })
	elseif k == "save" then
		local name = clean(req.name)
		if not name or name == PLAYROOM then return reply(ply, "error", "the Playroom profile can't be changed: save as another name") end
		save(name, { messages = req.profile and req.profile.messages or {}, taskparams = req.profile and req.profile.taskparams or {} })
		RunConsoleCommand("dms_tuning_profile", name)
		DMS_TuningApply()
	elseif k == "delete" then
		local name = clean(req.name)
		if not name or name == PLAYROOM then return end
		file.Delete(path(name))
		if profileName:GetString() == name then RunConsoleCommand("dms_tuning_profile", PLAYROOM) end
	elseif k == "use" then
		RunConsoleCommand("dms_tuning_profile", clean(req.name) or PLAYROOM)
		RunConsoleCommand("dms_tuning", req.on and "1" or "0")
	elseif k == "reset" then
		-- Back to the Playroom: tuning off, the Playroom's profile.
		RunConsoleCommand("dms_tuning", "0")
		RunConsoleCommand("dms_tuning_profile", PLAYROOM)
	elseif k == "react" then
		-- A reaction or a message on the Homer the menu picked (live try-out).
		local ent = Entity(tonumber(req.ent) or -1)
		if IsValid(ent) and ent.DMS_Agent then
			if req.line then DMS_Send(ent, req.line) else DMS_React(ent, req.react or "") end
		end
		return
	elseif k == "switch" then
		local cv = GetConVar("dms_" .. tostring(req.name))
		if cv and req.name:match("^[%w_]+$") then RunConsoleCommand("dms_" .. req.name, tostring(req.value)) end
		return
	end
	timer.Simple(0, function() if IsValid(ply) then reply(ply, "catalogue", catalogue()) end end)
end)

-- dms_tuning_status: the active profile's values.
concommand.Add("dms_tuning_status", function(ply)
	if IsValid(ply) and not ply:IsSuperAdmin() then return end
	local name = profileName:GetString()
	local t = DMS_TuningLoad(name)
	local lines = { string.format("[DMS] tuning %s, profile %s%s", tuningOn:GetBool() and "on" or "off (the Playroom's)", name, t and "" or " (missing)") }
	for msg, ps in SortedPairs(t and t.messages or {}) do
		for p, v in SortedPairs(ps) do lines[#lines + 1] = string.format("  %s %s = %s", msg, p, tostring(v)) end
	end
	for key, v in SortedPairs(t and t.taskparams or {}) do lines[#lines + 1] = string.format("  taskparams %s = %s", key, tostring(v)) end
	DMS_Say(ply, table.concat(lines, "\n"))
end)
