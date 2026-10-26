-- The control and telemetry channel of tools/dms-ctl (ROADMAP M2.5): a TCP server of the module on 127.0.0.1,
-- only while dms_dev is 1 (off by default), behind dms_dev_token. The bench (scripts\server.ps1) turns it on from
-- data/dms/bench.txt, which it writes for that one start.
--
-- Console commands and Lua from dms-ctl run here, in Think; NM messages and scenarios go to the module's bench agent.

local METRES_PER_UNIT = 0.0254

local dev = CreateConVar("dms_dev", "0", FCVAR_PROTECTED + FCVAR_DONTRECORD,
	"1: open the dms-ctl channel on 127.0.0.1 (development only, needs dms_dev_token)")
local port = CreateConVar("dms_dev_port", "27099", FCVAR_PROTECTED + FCVAR_DONTRECORD, "The dms-ctl channel's port")
local token = CreateConVar("dms_dev_token", "", FCVAR_PROTECTED + FCVAR_DONTRECORD + FCVAR_UNLOGGED,
	"The dms-ctl channel's token (8+ characters), set anew on each start")

-- Lua from dms-ctl: as a chunk, or as an expression if it isn't one; what it prints and returns, tab separated.
local function runLua(code)
	local fn = CompileString(code, "dms-ctl", false)
	if type(fn) ~= "function" then
		local expr = CompileString("return " .. code, "dms-ctl", false)
		if type(expr) ~= "function" then return false, tostring(fn) end
		fn = expr
	end
	local out, oldPrint = {}, print
	print = function(...)
		local parts = {}
		for i = 1, select("#", ...) do parts[i] = tostring((select(i, ...))) end
		out[#out + 1] = table.concat(parts, "\t")
		oldPrint(...)
	end
	local r = { pcall(fn) }
	print = oldPrint
	if not r[1] then
		out[#out + 1] = tostring(r[2])
		return false, table.concat(out, "\n")
	end
	local results = {}
	for i = 2, table.maxn(r) do results[#results + 1] = tostring(r[i]) end
	if #results > 0 then out[#out + 1] = table.concat(results, "\t") end
	return true, table.concat(out, "\n")
end

-- A FOLLOW scenario (ROADMAP M4): Homer at the Playroom room's start (`up` m higher for its `fall`), facing as there,
-- the player where the Playroom's camera starts (4 m back, 1.5 m up, looking at him); then the module runs it.
-- `r.text`: `<up> <stage> <start x y z>` (m; the stage's start, where the Playroom puts Homer on it, ROADMAP M5).
local function benchSpawn(r)
	local w = string.Explode(" ", r.text)
	local stage = w[2] or "room"
	local start = Vector(tonumber(w[3]) or 0, tonumber(w[4]) or 0, tonumber(w[5]) or 0) / METRES_PER_UNIT
	local scene, err = DMS.BenchScene(stage)
	if not scene then error("bench scene: " .. err) end
	DMS_MapWorld()
	for _, e in ipairs(ents.FindByModel(DMS_HOMER_MODEL)) do
		if e.DMS_Bench then
			DMS_Unfollow(e)
			e:Remove()
		end
	end
	-- The player out of the way of the stage's boxes (4 m back in the room, 8 m on a stage).
	local back = stage == "room" and 4 or 8
	for _, p in ipairs(player.GetAll()) do
		p:SetPos(scene.origin + Vector(0, -back / METRES_PER_UNIT, 0))
		p:SetEyeAngles(Angle(10, 90, 0))
	end
	local up = tonumber(w[1]) or 0
	local ent, why = DMS_SpawnHomer(scene.origin + start + Vector(0, 0, up / METRES_PER_UNIT), 0)
	if not ent then error(why) end
	ent.DMS_Bench = true
	local id = DMS.BenchFollow(DMS_HomerParts(ent), scene.origin * METRES_PER_UNIT)
	ent.DMS_Agent = id
	DMS_Agents[id] = ent
	ent:CallOnRemove("DMS_Unfollow", function(e) DMS_Unfollow(e) end)
end

local function answer(r)
	if r.kind == "exec" then
		game.ConsoleCommand(r.text .. "\n")
		return true, "queued for the server console (its output goes to the console log; dms-ctl exec uses RCON for it)"
	end
	return runLua(r.text)
end

local function think()
	local g = physenv.GetGravity().z * METRES_PER_UNIT
	for _, r in ipairs(DMS.DevThink(CurTime(), engine.TickCount(), FrameTime(), -g)) do
		local ok, ran, text = pcall(r.kind == "bench_spawn" and benchSpawn or answer, r)
		if r.kind == "bench_spawn" then
			-- The module answers as the scenario ends; only a failure to start is answered here.
			if not ok then DMS.DevReply(r.conn, r.id, false, "the bench's Homer: " .. tostring(ran)) end
		elseif ok then
			DMS.DevReply(r.conn, r.id, ran, text or "")
		else
			DMS.DevReply(r.conn, r.id, false, tostring(ran))
		end
	end
end

local function apply()
	if not dev:GetBool() then
		if DMS.DevPort() then
			DMS.DevStop()
			print("[DMS] dms-ctl channel off")
		end
		hook.Remove("Think", "DMS_Dev")
		return
	end
	if #token:GetString() < 8 then
		print("[DMS] dms_dev 1 needs dms_dev_token (8+ characters): the dms-ctl channel stays off")
		return
	end
	local ok, err = pcall(DMS.DevStart, port:GetInt(), token:GetString())
	if not ok then
		print("[DMS] dms-ctl channel: " .. tostring(err))
		return
	end
	hook.Add("Think", "DMS_Dev", think)
	print("[DMS] dms-ctl channel on 127.0.0.1:" .. port:GetInt())
	-- The bench agent needs NM's data.
	if not DMS.DataInfo() then
		local loaded, info = pcall(DMS.LoadData, GetConVar("dms_game_dir"):GetString())
		print(loaded and ("[DMS] NM's data: " .. info.dir) or ("[DMS] NM's data isn't loaded: " .. tostring(info)))
	end
end

-- The convars are checked every second (their change callbacks don't fire for every way they're set).
local applied
local function check()
	local now = dev:GetBool() and (port:GetInt() .. " " .. token:GetString()) or "off"
	if now == applied then return end
	applied = now
	apply()
end
timer.Create("DMS_Dev", 1, 0, check)

hook.Add("ShutDown", "DMS_Dev", function()
	if DMS.DevPort() then DMS.DevStop() end
end)

-- The bench's settings (scripts\server.ps1 start), once these convars exist (a +dms_dev on the command line would
-- come too early, and GMod blocks exec from Lua): data/dms/bench.txt's dms_* lines, `name value` or `name "value"`.
-- The file goes as soon as it's read, so a later normal game never opens the channel.
local bench = file.Read("dms/bench.txt", "DATA")
if bench then
	file.Delete("dms/bench.txt")
	for line in bench:gmatch("[^\r\n]+") do
		local name, value = line:match('^%s*(dms_[%w_]+)%s+"(.-)"%s*$')
		if not name then name, value = line:match("^%s*(dms_[%w_]+)%s+(%S+)%s*$") end
		if name then RunConsoleCommand(name, value) end
	end
end
check()
