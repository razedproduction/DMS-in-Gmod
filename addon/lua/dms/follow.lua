-- Homer in FOLLOW (ROADMAP M4): NM drives the ragdoll's 21 bodies from the module's step callback (module/src/follow.rs).
-- Lua puts NM on a bound Homer (DMS_Follow), takes it off before the ragdoll goes, tells the module Source's gravity
-- and hears of the agents in Think (docs/adr/0003): feedback, falls, sleep, NaN.

local METRES_PER_UNIT = 0.0254
local say = DMS_Say

-- docs/adr/0007: how Source's gravity is made up to NM's 9.8 m/s^2 (sv_gravity is never touched).
local gravityMode = CreateConVar("dms_gravity_mode", "nm", FCVAR_ARCHIVE,
	"How Homer's gravity is made up to NM's 9.8 m/s^2: nm (always), hybrid (only touching the world), off")

-- docs/adr/0021: how many Homers NM drives at once (NM's step costs 1-2 ms each), and on how many threads.
local maxAgents = CreateConVar("dms_max_agents", "8", FCVAR_ARCHIVE,
	"Homers NM drives at most; more stay ordinary ragdolls (NM's step costs 1-2 ms each)")
local threads = CreateConVar("dms_threads", "0", FCVAR_ARCHIVE,
	"Threads NM's agents step on at once (0: the CPU's cores less one, 1: the physics' thread only)")
local function setThreads() DMS.SetThreads(math.max(threads:GetInt(), 0)) end
cvars.AddChangeCallback("dms_threads", setThreads, "DMS_Threads")
setThreads()

-- Agent id -> entity.
DMS_Agents = DMS_Agents or {}

local function mode()
	local m = gravityMode:GetString()
	if m ~= "nm" and m ~= "hybrid" and m ~= "off" then return "nm" end
	return m
end

-- NM on Homer's bound ragdoll (DMS_SpawnHomer), facing as he was spawned. The agent's id, or nil and why.
function DMS_Follow(ent)
	if ent.DMS_Agent then return ent.DMS_Agent end
	local n = 0
	for _, e in pairs(DMS_Agents) do
		-- Removed this frame: its agent goes as the frame ends.
		if IsValid(e) and not e:IsMarkedForDeletion() then n = n + 1 end
	end
	if n >= maxAgents:GetInt() then return nil, string.format("dms_max_agents (%d) Homers have NM already", maxAgents:GetInt()) end
	local parts, why = DMS_HomerParts(ent)
	if not parts then return nil, why end
	local s = ent.DMS_Spawn or { pos = ent:GetPos(), yaw = ent:GetAngles().y }
	-- NM's world: the map's (ROADMAP M5), or the Playroom's floor at his feet' level.
	DMS_MapWorld()
	local ok, id = pcall(DMS.Follow, parts, math.rad(s.yaw), s.pos * METRES_PER_UNIT, false)
	if not ok then return nil, tostring(id) end
	ent.DMS_Agent = id
	DMS_Agents[id] = ent
	-- Off before the ragdoll's bodies go.
	ent:CallOnRemove("DMS_Unfollow", function(e) DMS_Unfollow(e) end)
	hook.Run("DMS_Attached", ent, id)
	return id
end

-- The agent is gone (taken off, or came off by itself): Lua forgets it; a Homer held on his get-up clip moves again
-- (docs/adr/0021).
local function forget(ent, id, why)
	DMS_Agents[id] = nil
	if not IsValid(ent) then return end
	ent.DMS_Agent, ent.DMS_Lod, ent.DMS_StillFor = nil, nil, nil
	if ent.DMS_Rising then DMS_Unfreeze(ent) end
	hook.Run("DMS_Detached", ent, why)
end

-- NM off the ragdoll: an ordinary ragdoll again.
function DMS_Unfollow(ent)
	local id = ent.DMS_Agent
	if not id then return end
	-- On a map's change the module has dropped its agents already (and DMS may be gone).
	if DMS then DMS.Unfollow(id) end
	forget(ent, id, "detached")
end

-- A message line to Homer's agent (`bodyBalance start=true`, as the Playroom's console takes it).
function DMS_Send(ent, line)
	if not ent.DMS_Agent then return false, "no NM on it" end
	DMS_Wake(ent)
	return pcall(DMS.FollowSend, ent.DMS_Agent, line)
end

local function partsOf(id)
	local ent = DMS_Agents[id]
	if not IsValid(ent) then return end
	return ent, DMS_HomerParts(ent)
end

-- By event kind; reactions.lua adds the room's (freeze, unfreeze, died, say).
DMS_FollowHandlers = DMS_FollowHandlers or {}
local handlers = DMS_FollowHandlers
for k, h in pairs({
	-- NM's doll fell asleep: so do the bodies (no drives come while it sleeps); a hit wakes both.
	sleep = function(ent, parts) for _, p in ipairs(parts) do p:Sleep() end end,
	feedback = function(ent, _, e)
		local name, slot = e.text:match("^(%S+) (%d+)$")
		hook.Run("DMS_Feedback", ent, name, tonumber(slot), e.value)
	end,
	fallen = function(ent) hook.Run("DMS_Fallen", ent) end,
	detached = function(ent, _, e)
		print("[DMS] NM came off " .. tostring(ent) .. ": " .. e.text)
		forget(ent, e.id, e.text)
	end,
	-- The game moved him (SetPos): NM's frame went with him (docs/adr/0021).
	teleported = function(ent, _, e)
		local s = DMS.FollowState(e.id)
		if s and ent.DMS_Spawn then ent.DMS_Spawn.pos = Vector(s.origin.x, s.origin.y, s.origin.z) / METRES_PER_UNIT end
		hook.Run("DMS_Teleported", ent, e.value)
	end,
	-- A parked agent's bodies moved: NM is back on them (lifecycle.lua).
	unparked = function(ent) ent.DMS_Lod, ent.DMS_StillFor = nil, nil end,
}) do handlers[k] = h end

local nextMask = 0
hook.Add("Think", "DMS_Follow", function()
	local g = physenv.GetGravity() * METRES_PER_UNIT
	DMS.SetGravity(g, mode())
	for _, e in ipairs(DMS.FollowEvents()) do
		local ent, parts = partsOf(e.id)
		local h = handlers[e.kind]
		if ent and parts and h then h(ent, parts, e) end
	end
	-- docs/adr/0007: parts the game doesn't pull get no Source gravity to make up.
	if CurTime() < nextMask then return end
	nextMask = CurTime() + 0.25
	for id, ent in pairs(DMS_Agents) do
		local parts = IsValid(ent) and DMS_HomerParts(ent)
		if parts then
			local mask, frozen = 0, false
			for i, p in ipairs(parts) do
				if not p:IsGravityEnabled() then mask = mask + 2 ^ (i - 1) end
				frozen = frozen or not p:IsMotionEnabled()
			end
			-- The module took NM off by itself (the bench's scenario ended): forget the agent.
			if not DMS.FollowGravityOff(id, mask) then
				forget(ent, id, "detached")
			-- docs/adr/0021: a part frozen by the player (physgun, a tool) parks NM till it's unfrozen; the get-up
			-- clip's own freeze isn't.
			elseif frozen and not ent.DMS_Rising and ent.DMS_Lod ~= "frozen" and not ent.DMS_Bench then
				DMS.FollowPark(id, true, false)
				ent.DMS_Lod = "frozen"
			elseif not frozen and ent.DMS_Lod == "frozen" then
				DMS.FollowPark(id, false)
				ent.DMS_Lod = nil
			end
		else
			-- Gone (its OnRemove unfollowed it), or its bodies can't be told apart any more: NM off.
			if IsValid(ent) then DMS_Unfollow(ent) end
			DMS_Agents[id] = nil
		end
	end
end)

local function admin(ply) return not IsValid(ply) or ply:IsSuperAdmin() end

-- dms_status: the FOLLOW agents and what they cost.
concommand.Add("dms_status", function(ply)
	if not admin(ply) then return end
	local st = DMS.FollowStats()
	local lines = { string.format("[DMS] %d agent(s) (dms_max_agents %d), gravity %s (Source %.2f m/s^2, NM 9.8), NM %.3f ms a physics step on %d thread(s)",
		#DMS.FollowList(), maxAgents:GetInt(), mode(), -physenv.GetGravity().z * METRES_PER_UNIT, st.ms, st.threads) }
	-- docs/adr/0009: with tuning on, Homer isn't the Playroom's.
	local tuning = GetConVar("dms_tuning")
	lines[#lines + 1] = (tuning and tuning:GetBool()) and ("  tuning ON, profile " .. GetConVar("dms_tuning_profile"):GetString())
		or "  tuning off (the Playroom's)"
	for _, id in ipairs(DMS.FollowList()) do
		local s = DMS.FollowState(id)
		local ent = DMS_Agents[id]
		if s then
			lines[#lines + 1] = string.format("  #%d %s: step %d, %s%s%s%s, pelvis z %.3f m, head z %.3f m, compensation %.2f, NM %.3f ms a step",
				id, tostring(ent), s.steps, s.balancing and "balancing" or "not balancing", s.fallen and ", fallen" or "",
				s.asleep and ", asleep" or (s.awake and "" or ", still"),
				s.parked and (", parked (" .. tostring(IsValid(ent) and ent.DMS_Lod or "?") .. ")") or "",
				s.pelvis.z, s.head.z, s.compensation, s.cost)
		end
	end
	say(ply, table.concat(lines, "\n"))
end)

-- dms_follow_send <message line>: to every FOLLOW Homer (e.g. dms_follow_send bodyRelax start=true).
concommand.Add("dms_follow_send", function(ply, _, _, line)
	if not admin(ply) then return end
	for _, ent in pairs(DMS_Agents) do
		if IsValid(ent) then
			local ok, err = DMS_Send(ent, line)
			if not ok then return say(ply, "[DMS] " .. tostring(err)) end
		end
	end
end)
