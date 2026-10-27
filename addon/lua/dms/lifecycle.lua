-- Homer's agents through the game's life (ROADMAP M7, docs/adr/0021): copies (the duplicator, saves) are bound and
-- get NM again; LOD parks NM on Homers far from every player or lying still; the get-up clip is given up when the
-- player takes him off it. Removal, undo, cleanup and the map's change unfollow through OnRemove and ShutDown
-- (follow.lua, dms_init.lua); frozen parts and teleports are follow.lua's and the module's.

local M = 0.0254

local lodDistance = CreateConVar("dms_lod_distance", "3000", FCVAR_ARCHIVE,
	"Homers farther than this from every player (units) relax, and NM comes off them till one comes near (0: never)")
local lodStill = CreateConVar("dms_lod_still", "5", FCVAR_ARCHIVE,
	"Homers lying still this long (s) have NM taken off till something moves them (0: never)")
-- How long a far Homer relaxes before NM comes off him (s); how slowly a still one's centre of mass goes (m/s).
local RELAX_FOR = 1
local STILL = 0.02

-- NM back on a Homer parked far away or lying still, before a reaction or a message reaches him. A frozen one stays
-- parked till he's unfrozen (follow.lua).
function DMS_Wake(ent)
	local lod = ent.DMS_Lod
	if not lod or lod == "frozen" then return end
	ent.DMS_Lod, ent.DMS_StillFor = nil, nil
	if lod ~= "relaxing" and ent.DMS_Agent then DMS.FollowPark(ent.DMS_Agent, false) end
end

-- The duplicator and saves (gm_save is duplicator.CopyEnts, then game.CleanUpMap and duplicator.Paste) merge an
-- entity's Lua table into its copy. DMS's fields are this Homer's (his agent, his callbacks, his spawn): they stay out
-- of it, only DMS_Copy goes, and the copy is bound and gets NM again (adopt below).
local held = setmetatable({}, { __mode = "k" })

function DMS_PreCopy(ent)
	local t = {}
	for k, v in pairs(ent:GetTable()) do
		if isstring(k) and k:sub(1, 4) == "DMS_" then t[k] = v end
	end
	for k in pairs(t) do ent[k] = nil end
	held[ent] = t
	ent.DMS_Copy = t.DMS_Agent and "attached" or "bound"
end

function DMS_PostCopy(ent)
	ent.DMS_Copy = nil
	for k, v in pairs(held[ent] or {}) do ent[k] = v end
	held[ent] = nil
end

-- A Homer that turns up without his spawn's binding: a copy (DMS_Copy), a copy from a save older than this (an agent
-- that isn't his), or one made elsewhere. He's bound as he is (the binding doesn't depend on the pose), and gets NM if
-- the Homer he copies had it, or with dms_autoattach.
local function forgetFields(ent)
	local keys = {}
	for k in pairs(ent:GetTable()) do
		if isstring(k) and k:sub(1, 4) == "DMS_" then keys[#keys + 1] = k end
	end
	for _, k in ipairs(keys) do ent[k] = nil end
end

-- A copy of a ValveBiped ragdoll NM drove (ROADMAP M11): its objects come frozen (as NM held the original's), so its
-- own physics first, then NM again; a copy from a save older than this has the original's fields (`stale`).
local function adoptRagdoll(ent)
	local copy = ent.DMS_Copy
	local stale = ent.DMS_Agent ~= nil and DMS_Agents[ent.DMS_Agent] ~= ent
	if copy ~= "attached" and not stale then return end
	forgetFields(ent)
	for i = 0, ent:GetPhysicsObjectCount() - 1 do
		local p = ent:GetPhysicsObjectNum(i)
		if IsValid(p) then
			p:EnableMotion(true)
			p:EnableCollisions(true)
			p:EnableGravity(true)
			p:Wake()
		end
	end
	local c, why = dms.Attach(ent)
	if not c then print("[DMS] NM isn't on a ragdoll's copy: " .. tostring(why)) end
end

-- The mark a ValveBiped ragdoll with NM leaves in its copies (ragdolls.lua): a ragdoll's paste doesn't carry its Lua
-- table, only its modifiers.
duplicator.RegisterEntityModifier("DMS_NM", function(_, ent) ent.DMS_Copy = "attached" end)

local function adopt(ent)
	if DMS_IsValveBiped(ent) then return adoptRagdoll(ent) end
	if not IsValid(ent) or ent:GetModel() ~= DMS_HOMER_MODEL then return end
	local copy = ent.DMS_Copy
	local stale = ent.DMS_Agent ~= nil and DMS_Agents[ent.DMS_Agent] ~= ent
	if not copy and not stale and ent.DMS_Bound then return end
	local rising = ent.DMS_Rising
	forgetFields(ent)
	-- A copy of one held on his get-up clip: his bodies were frozen with it.
	if rising then
		for _, p in ipairs(DMS_HomerParts(ent) or {}) do
			p:EnableMotion(true)
			p:Wake()
		end
	end
	local ok, why = DMS_BindRagdoll(ent)
	if not ok then return print("[DMS] a Homer copy isn't bound: " .. tostring(why)) end
	if copy == "attached" or stale or GetConVar("dms_autoattach"):GetBool() then
		local c, err = dms.Attach(ent)
		if not c then print("[DMS] NM isn't on a Homer copy: " .. tostring(err)) end
	end
end

hook.Add("OnEntityCreated", "DMS_Lifecycle", function(ent)
	-- Its model and the duplicator's fields are there by the next tick.
	if ent:GetClass() == "prop_ragdoll" then timer.Simple(0, function() adopt(ent) end) end
end)

-- The player takes him off his get-up clip: the clip is given up, NM takes the bodies as they are; a parked Homer
-- gets NM back.
local function taken(ply, ent)
	if not IsValid(ent) or not ent.DMS_Agent then return end
	if ent.DMS_Rising then
		DMS.FollowRiseCancel(ent.DMS_Agent)
		DMS_Unfreeze(ent)
	end
	DMS_Wake(ent)
end
hook.Add("PhysgunPickup", "DMS_Lifecycle", function(ply, ent) taken(ply, ent) end)
hook.Add("GravGunOnPickedUp", "DMS_Lifecycle", taken)

-- LOD, twice a second: far from every player he relaxes for a second and NM comes off him (back once one comes near);
-- lying still for dms_lod_still s NM comes off him (back once something moves him: the module tells, `unparked`).
local STEP = 0.5
local nextLod = 0
hook.Add("Think", "DMS_Lifecycle", function()
	if CurTime() < nextLod then return end
	nextLod = CurTime() + STEP
	local far, still = lodDistance:GetFloat(), lodStill:GetFloat()
	local players = player.GetAll()
	for id, ent in pairs(DMS_Agents) do
		local lod = IsValid(ent) and ent.DMS_Lod
		if IsValid(ent) and not ent.DMS_Bench and lod ~= "frozen" and lod ~= "still" then
			local s, r = DMS.FollowState(id), DMS.FollowRoom(id)
			if s and r then
				local pelvis = Vector(s.pelvis.x, s.pelvis.y, s.pelvis.z) / M
				local near = far <= 0
				for _, p in ipairs(players) do
					if near then break end
					near = p:GetPos():DistToSqr(pelvis) < far * far
				end
				if not near then
					if not lod then
						dms.Get(ent):Relax()
						ent.DMS_Lod, ent.DMS_LodAt = "relaxing", CurTime() + RELAX_FOR
					elseif lod == "relaxing" and CurTime() >= ent.DMS_LodAt then
						DMS.FollowPark(id, true, false)
						ent.DMS_Lod = "far"
					end
				elseif lod then
					DMS_Wake(ent)
				elseif still > 0 then
					-- By how far he went: lying under NM, the bodies' velocities jitter about 0.1 m/s on the spot.
					local com, last = Vector(s.com.x, s.com.y, s.com.z), ent.DMS_StillAt
					ent.DMS_StillAt = com
					if s.balancing or r.rising or not last or com:Distance(last) > STILL * STEP then
						ent.DMS_StillFor = 0
					else
						ent.DMS_StillFor = (ent.DMS_StillFor or 0) + STEP
						if ent.DMS_StillFor >= still then
							DMS.FollowPark(id, true, true)
							ent.DMS_Lod = "still"
						end
					end
				end
			end
		end
	end
end)
