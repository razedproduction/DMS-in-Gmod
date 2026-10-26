-- Homer in FOLLOW (ROADMAP M4): NM drives the ragdoll's 21 bodies from the module's step callback (module/src/follow.rs).
-- Lua puts NM on a bound Homer (DMS_Follow), takes it off before the ragdoll goes, tells the module Source's gravity
-- and hears of the agents in Think (docs/adr/0003): feedback, falls, sleep, NaN.

local METRES_PER_UNIT = 0.0254
local say = DMS_Say

-- docs/adr/0007: how Source's gravity is made up to NM's 9.8 m/s^2 (sv_gravity is never touched).
local gravityMode = CreateConVar("dms_gravity_mode", "nm", FCVAR_ARCHIVE,
	"How Homer's gravity is made up to NM's 9.8 m/s^2: nm (always), hybrid (only touching the world), off")

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
	local parts, why = DMS_HomerParts(ent)
	if not parts then return nil, why end
	local s = ent.DMS_Spawn or { pos = ent:GetPos(), yaw = ent:GetAngles().y }
	local ok, id = pcall(DMS.Follow, parts, math.rad(s.yaw), s.pos * METRES_PER_UNIT, false)
	if not ok then return nil, tostring(id) end
	ent.DMS_Agent = id
	DMS_Agents[id] = ent
	-- Off before the ragdoll's bodies go.
	ent:CallOnRemove("DMS_Unfollow", function(e) DMS_Unfollow(e) end)
	return id
end

-- NM off the ragdoll: an ordinary ragdoll again.
function DMS_Unfollow(ent)
	local id = ent.DMS_Agent
	if not id then return end
	DMS.Unfollow(id)
	DMS_Agents[id] = nil
	ent.DMS_Agent = nil
end

-- A message line to Homer's agent (`bodyBalance start=true`, as the Playroom's console takes it).
function DMS_Send(ent, line)
	if not ent.DMS_Agent then return false, "no NM on it" end
	return pcall(DMS.FollowSend, ent.DMS_Agent, line)
end

local function partsOf(id)
	local ent = DMS_Agents[id]
	if not IsValid(ent) then return end
	return ent, DMS_HomerParts(ent)
end

local handlers = {
	-- NM's doll fell asleep: so do the bodies (no drives come while it sleeps); a hit wakes both.
	sleep = function(ent, parts) for _, p in ipairs(parts) do p:Sleep() end end,
	feedback = function(ent, _, e)
		local name, slot = e.text:match("^(%S+) (%d+)$")
		hook.Run("DMS_Feedback", ent, name, tonumber(slot), e.value)
	end,
	fallen = function(ent) hook.Run("DMS_Fallen", ent) end,
	detached = function(ent, _, e)
		print("[DMS] NM came off " .. tostring(ent) .. ": " .. e.text)
		DMS_Agents[e.id] = nil
		ent.DMS_Agent = nil
		hook.Run("DMS_Detached", ent, e.text)
	end,
}

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
			local mask = 0
			for i, p in ipairs(parts) do
				if not p:IsGravityEnabled() then mask = mask + 2 ^ (i - 1) end
			end
			-- The module took NM off by itself (the bench's scenario ended): forget the agent.
			if not DMS.FollowGravityOff(id, mask) then
				DMS_Agents[id] = nil
				ent.DMS_Agent = nil
			end
		else
			DMS_Agents[id] = nil
		end
	end
end)

local function admin(ply) return not IsValid(ply) or ply:IsSuperAdmin() end

-- dms_status: the FOLLOW agents and what they cost.
concommand.Add("dms_status", function(ply)
	if not admin(ply) then return end
	local lines = { string.format("[DMS] %d agent(s), gravity %s (Source %.2f m/s^2, NM 9.8), NM %.3f ms a physics step",
		#DMS.FollowList(), mode(), -physenv.GetGravity().z * METRES_PER_UNIT, DMS.FollowCost()) }
	for _, id in ipairs(DMS.FollowList()) do
		local s = DMS.FollowState(id)
		local ent = DMS_Agents[id]
		if s then
			lines[#lines + 1] = string.format("  #%d %s: step %d, %s%s%s, pelvis z %.3f m, head z %.3f m, compensation %.2f",
				id, tostring(ent), s.steps, s.balancing and "balancing" or "not balancing", s.fallen and ", fallen" or "",
				s.asleep and ", asleep" or (s.awake and "" or ", still"), s.pelvis.z, s.head.z, s.compensation)
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
