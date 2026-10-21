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
		local ok, ran, text = pcall(answer, r)
		if ok then
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
