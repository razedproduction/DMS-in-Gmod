-- The Playroom room's reactions on Homer (ROADMAP M6, docs/adr/0016, 0020): the game's damage, near misses, throws
-- and drops become the module's reactions (DMS.FollowReact, the Playroom's own code in module/src/follow/room.rs);
-- the Playroom's switches are convars with its defaults; Homer is held on his get-up clip while he gets up by
-- animation. Only the translation of the game's events is ours: part, point, direction, weapon.

local M = 0.0254
local say = DMS_Say

-- The Playroom's switches (settings.txt, docs/adr/0016): name, convar default, help.
local SWITCHES = {
	{ "tasks", "0", "NM -> animation as the game's tasks end (the Playroom's game tasks)" },
	{ "look", "1", "He looks at what's thrown at him" },
	{ "avoid", "0", "He flinches from what's thrown at him" },
	{ "getup", "0", "When he lies still: 0 the game's get-up clip, 1 his muscles, 2 he stays down, 3 learned (not ported)" },
	{ "extra_reactions", "0", "The Playroom's additional reactions (dms_extra_pick picks them)" },
	{ "extra_pick", "255", "Additional reactions, a bit each: 1 windmill, 2 curl up before getting up, 4 curl up when shot at "
		.. "lying, 8 look at who hit him, 16 tumble off a ledge, 32 smartFall when dropped, 64 dodge from above, 128 brace "
		.. "on a wall" },
	{ "fire_reaction", "1", "NM's OnFire task when he burns" },
	{ "dive_shot", "0", "Shot at again, he dives" },
	{ "dodge_gap", "3", "Seconds between his dives" },
	{ "weak", "0", "The weapons' weaker taskparams sets (agility 2)" },
	{ "god", "0", "Infinite health" },
}

local convars = {}
for _, s in ipairs(SWITCHES) do
	convars[s[1]] = CreateConVar("dms_" .. s[1], s[2], { FCVAR_ARCHIVE, FCVAR_REPLICATED }, s[3])
end

local function pushSwitches()
	if not DMS.SetReactions then return end
	local kv = {}
	for _, s in ipairs(SWITCHES) do
		kv[#kv + 1] = s[1] .. "=" .. convars[s[1]]:GetString()
	end
	local ok, err = pcall(DMS.SetReactions, table.concat(kv, " "))
	if not ok then print("[DMS] reactions: " .. tostring(err)) end
end
for _, s in ipairs(SWITCHES) do
	cvars.AddChangeCallback("dms_" .. s[1], pushSwitches, "DMS_Reactions")
end
pushSwitches()

local logReactions = CreateConVar("dms_react_log", "0", FCVAR_ARCHIVE, "Print the room's reactions (the Playroom's log lines)")

-- A point (units -> m) and a direction as the module takes them.
local function at(p) return string.format("%.4f,%.4f,%.4f", p.x * M, p.y * M, p.z * M) end
local function dir(d) return string.format("%.4f,%.4f,%.4f", d.x, d.y, d.z) end

-- A reaction on a Homer with NM (`DMS.FollowReact`'s line). True if it went.
function DMS_React(ent, line)
	if not IsValid(ent) or not ent.DMS_Agent then return false, "no NM on it" end
	-- Parked far away or lying still (lifecycle.lua): NM back on him first.
	DMS_Wake(ent)
	local ok, res = pcall(DMS.FollowReact, ent.DMS_Agent, line)
	if not ok then return false, res end
	return res
end

-- The game's weapons as the Playroom's (its guns' taskparams sets and weaponinfo.xml's damage).
local WEAPONS = {
	weapon_pistol = "pistol", weapon_357 = "deagle", weapon_smg1 = "smg", weapon_ar2 = "ak47",
	weapon_crossbow = "sniper", weapon_shotgun = "shotgun", weapon_rpg = "rpg", weapon_frag = "grenade",
	weapon_crowbar = "bat", weapon_stunstick = "baton",
}

local function weaponOf(dmg)
	local inf, att = dmg:GetInflictor(), dmg:GetAttacker()
	local w = IsValid(inf) and inf:IsWeapon() and inf or (IsValid(att) and att.GetActiveWeapon and att:GetActiveWeapon())
	local name = IsValid(w) and WEAPONS[w:GetClass()]
	if name then return name end
	if dmg:IsDamageType(DMG_BUCKSHOT) then return "shotgun" end
	return "pistol"
end
DMS_WeaponOf = weaponOf

-- Where a blast that hurt `ent` was: Source's RadiusDamage puts its centre in the damage position.
function DMS_BlastCentre(ent, dmg, pos)
	local inf = dmg:GetInflictor()
	local centre = dmg:GetDamagePosition()
	if centre:IsZero() then centre = dmg:GetReportedPosition() end
	if centre:IsZero() and IsValid(inf) and inf ~= ent then centre = inf:GetPos() end
	if centre:IsZero() then centre = pos end
	return centre
end

local function homerMass(ent)
	local m = 0
	for i = 0, ent:GetPhysicsObjectCount() - 1 do
		local p = ent:GetPhysicsObjectNum(i)
		if IsValid(p) then m = m + p:GetMass() end
	end
	return m
end

-- One shot a tick for each Homer: a shotgun's pellets are one shot (the Playroom's `pellets`), its other parts `more`.
local pending = {}
local function flushShots()
	for ent, s in pairs(pending) do
		local line = string.format("shot at=%s dir=%s weapon=%s", at(s.pos), dir(s.dir), s.weapon)
		-- On a ragdoll: the NM part of the physics bone the bullet hit (ragdolls.lua).
		if s.part then line = line .. " part=" .. s.part end
		if #s.more > 0 then line = line .. " more=" .. table.concat(s.more, ",") end
		DMS_React(ent, line)
	end
	pending = {}
end

-- The part NM's doll has nearest to a point (its capsules), for the shotgun's other pellets: the module picks the
-- first's itself.
local function partNear(ent, pos, d)
	if ent.DMS_Native then return DMS_RagdollPartAt(ent, pos, d) or 9 end
	local parts = DMS_HomerParts(ent)
	if not parts then return 9 end
	local best, bd = 9, math.huge
	for i, p in ipairs(parts) do
		local d = p:LocalToWorld(p:GetMassCenter()):DistToSqr(pos)
		if d < bd then best, bd = i - 1, d end
	end
	return best
end

-- A bullet on a body with NM, to go with the tick's others as one shot: at `pos` along `d`.
function DMS_QueueShot(ent, pos, d, weapon)
	local s = pending[ent]
	if s then
		s.more[#s.more + 1] = partNear(ent, pos, d)
	else
		if not next(pending) then timer.Simple(0, flushShots) end
		pending[ent] = { pos = pos, dir = d:GetNormalized(), weapon = weapon, more = {}, part = ent.DMS_Native and partNear(ent, pos, d) }
	end
end

local lastBlast = {}

hook.Add("EntityTakeDamage", "DMS_Reactions", function(ent, dmg)
	if not ent.DMS_Agent then return end
	local pos = dmg:GetDamagePosition()
	if pos:IsZero() then pos = ent:WorldSpaceCenter() end
	local force = dmg:GetDamageForce()
	local att, inf = dmg:GetAttacker(), dmg:GetInflictor()
	if dmg:IsBulletDamage() or dmg:IsDamageType(DMG_BUCKSHOT) then
		local d = force:IsZero() and IsValid(att) and (pos - (att.EyePos and att:EyePos() or att:GetPos())) or force
		DMS_QueueShot(ent, pos, d, weaponOf(dmg))
		-- The bullet's push is the Playroom's applyImpulse (the module's): not the game's as well.
		dmg:SetDamageForce(vector_origin)
	elseif dmg:IsExplosionDamage() then
		-- One blast a tick (the game hurts each of his bodies).
		if lastBlast[ent] ~= engine.TickCount() then
			lastBlast[ent] = engine.TickCount()
			DMS_React(ent, "blast at=" .. at(DMS_BlastCentre(ent, dmg, pos)))
		end
		dmg:SetDamageForce(vector_origin)
	elseif dmg:IsDamageType(DMG_BURN) or dmg:IsDamageType(DMG_SLOWBURN) then
		DMS_React(ent, "burn")
	elseif dmg:IsDamageType(DMG_CLUB) or dmg:IsDamageType(DMG_SLASH) then
		local head = IsValid(att) and (att.EyePos and att:EyePos() or att:GetPos()) or pos
		DMS_React(ent, string.format("melee damage=%.2f at=%s impulse=%s head=%s", dmg:GetDamage(), at(pos), at(force), at(head)))
		if dmg:IsDamageType(DMG_SHOCK) then DMS_React(ent, "taser at=" .. at(pos)) end
		dmg:SetDamageForce(vector_origin)
	elseif dmg:IsDamageType(DMG_SHOCK) then
		DMS_React(ent, "taser at=" .. at(pos))
	end
end)

-- Contacts: a car (the Playroom's taxi: its impulse on him over 8 steps, a reaction at most once a second) or a
-- thing (`obj_hit`, once a second each). The impulse is what the contact did to his part.
local CAR_HIT = 8 / 60
local function contact(ent, data)
	local other = data.HitEntity
	if not ent.DMS_Agent or not IsValid(other) or other:IsWorld() or other == ent then return end
	local p = data.PhysObject
	if not IsValid(p) then return end
	local j = (data.OurNewVelocity - data.OurOldVelocity) * p:GetMass()
	if j:Length() * M < 1 then return end
	local now = CurTime()
	if other:IsVehicle() or other:GetClass():find("^prop_vehicle") then
		if (ent.DMS_CarNext or 0) > now then return end
		local h = ent.DMS_CarHit
		if not h then
			h = { j = Vector(), pos = data.HitPos, id = other:EntIndex(), player = IsValid(other.GetDriver and other:GetDriver()) }
			ent.DMS_CarHit = h
			timer.Simple(CAR_HIT, function()
				if not IsValid(ent) then return end
				ent.DMS_CarHit, ent.DMS_CarNext = nil, CurTime() + 1
				DMS_React(ent, string.format("car id=%d impulse=%s at=%s player=%d", h.id, at(h.j), at(h.pos), h.player and 1 or 0))
			end)
		end
		h.j:Add(j)
		return
	end
	ent.DMS_PropHit = ent.DMS_PropHit or {}
	if (ent.DMS_PropHit[other] or 0) > now then return end
	ent.DMS_PropHit[other] = now + 1
	DMS_React(ent, string.format("prop at=%s impulse=%s", at(data.HitPos), at(j)))
end

hook.Add("DMS_Detached", "DMS_Reactions", function(ent)
	if IsValid(ent) then
		ent:SetNW2Bool("DMS_Homer", false)
		ent:SetNW2Bool("DMS_Agent", false)
	end
end)

hook.Add("DMS_Attached", "DMS_Reactions", function(ent)
	if not ent.DMS_Contact then
		ent.DMS_Contact = ent:AddCallback("PhysicsCollide", contact)
	end
end)

-- A bullet that misses him (the Playroom's `near_miss`: the module tells how near).
hook.Add("EntityFireBullets", "DMS_Reactions", function(shooter, data)
	if not next(DMS_Agents) then return end
	local src, d = data.Src, data.Dir:GetNormalized()
	local tr = util.TraceLine({ start = src, endpos = src + d * (data.Distance or 56756), filter = shooter, mask = MASK_SHOT })
	for _, ent in pairs(DMS_Agents) do
		if IsValid(ent) and tr.Entity ~= ent then
			local c = ent:WorldSpaceCenter()
			local along = (c - src):Dot(d)
			-- In front of the shooter, and not stopped by a wall before it gets to him.
			if along > 0 and along <= tr.Fraction * (data.Distance or 56756) + 64 then
				DMS_React(ent, string.format("nearmiss from=%s dir=%s", at(src), dir(d)))
			end
		end
	end
end)

-- Thrown at him (physgun, gravity gun): the Playroom's `throw` (he looks, and flinches with avoid); Homer himself let
-- go of: its `fall` (smartFall with the additional reactions).
local function thrown(ply, prop)
	timer.Simple(0, function()
		if not IsValid(prop) then return end
		if prop.DMS_Agent then
			if prop:GetVelocity():Length() * M > 1 then DMS_React(prop, "dropped") end
			return
		end
		local v = prop:GetVelocity()
		if v:Length() * M < 3 then return end
		for _, ent in pairs(DMS_Agents) do
			local to = IsValid(ent) and ent:WorldSpaceCenter() - prop:GetPos()
			if to and to:Length() * M < 30 and to:GetNormalized():Dot(v:GetNormalized()) > 0.8 then
				DMS_React(ent, string.format("thrown from=%s vel=%s", at(prop:GetPos()), at(v)))
			end
		end
	end)
end
hook.Add("PhysgunDrop", "DMS_Reactions", thrown)
hook.Add("GravGunOnDropped", "DMS_Reactions", thrown)
hook.Add("GravGunPunt", "DMS_Reactions", thrown)

-- Getting up by animation: the bodies are held on the clip (frozen, set each tick), then let go to NM.
local function freeze(ent, parts)
	ent.DMS_Rising = true
	for _, p in ipairs(parts) do p:EnableMotion(false) end
end

function DMS_Unfreeze(ent)
	ent.DMS_Rising = nil
	local parts = DMS_HomerParts(ent)
	if not parts then return end
	for _, p in ipairs(parts) do
		p:EnableMotion(true)
		p:SetVelocityInstantaneous(vector_origin)
		p:SetAngleVelocityInstantaneous(vector_origin)
		p:Wake()
	end
end

local H = DMS_FollowHandlers
H.freeze = function(ent, parts, e)
	-- The clip was given up before Lua heard of it (the player took him, lifecycle.lua).
	local r = DMS.FollowRoom(e.id)
	if r and not r.rising then return end
	freeze(ent, parts)
end
H.unfreeze = function(ent, _, e)
	DMS_Unfreeze(ent)
	DMS.FollowRiseDone(e.id)
end
H.died = function(ent) hook.Run("DMS_Died", ent) end
H.say = function(ent, _, e)
	if logReactions:GetBool() then print(string.format("[DMS] %s %.2f %s", tostring(ent), e.value, e.text)) end
	hook.Run("DMS_Reaction", ent, e.text)
end

local function holdOnClip(ent, parts)
	local pose = DMS.FollowPose(ent.DMS_Agent)
	if not pose then return end
	for i, p in ipairs(parts) do
		local k = (i - 1) * 6
		local centre = Vector(pose[k + 1], pose[k + 2], pose[k + 3])
		p:SetAngles(Angle(pose[k + 4], pose[k + 5], pose[k + 6]))
		p:SetPos(centre - p:LocalToWorldVector(p:GetMassCenter()), true)
	end
end

-- The module's {x, y, z}.
local function V(t) return Vector(t.x, t.y, t.z) end

-- The face and the HUD on the clients: NM's view of each Homer, 20 times a second (cl_face.lua).
local nextNet = 0
hook.Add("Think", "DMS_Reactions", function()
	for _, ent in pairs(DMS_Agents) do
		if IsValid(ent) and ent.DMS_Rising then
			local parts = DMS_HomerParts(ent)
			if parts then holdOnClip(ent, parts) end
		end
	end
	if CurTime() < nextNet then return end
	nextNet = CurTime() + 0.05
	for id, ent in pairs(DMS_Agents) do
		local r = IsValid(ent) and DMS.FollowRoom(id)
		local s = r and DMS.FollowState(id)
		if r and s then
			-- Burning (the Playroom's fire) while the game has him on fire.
			if ent:IsOnFire() then DMS_React(ent, "burn") end
			-- The face and the overlay are Homer's (his model's bones); the HUD and the window take any agent.
			ent:SetNW2Bool("DMS_Homer", not ent.DMS_Native)
			ent:SetNW2Bool("DMS_Agent", true)
			if ent.DMS_Spawn then ent:SetNW2Float("DMS_Floor", ent.DMS_Spawn.pos.z) end
			ent:SetNW2Float("DMS_Health", r.health)
			ent:SetNW2Bool("DMS_Dead", r.dead)
			ent:SetNW2Bool("DMS_Rising", r.rising)
			ent:SetNW2Vector("DMS_V", V(s.com_v))
			local parts = DMS_HomerParts(ent)
			local head = parts and parts[13]
			if head then
				local mc = head:GetMassCenter()
				ent:SetNW2Int("DMS_HeadBone", ent:LookupBone(DMS.PartNames()[13]) or -1)
				ent:SetNW2Vector("DMS_FaceCentre", mc + V(r.face_centre) / M)
				ent:SetNW2Vector("DMS_FaceFwd", V(r.face_fwd))
				ent:SetNW2Vector("DMS_FaceUp", V(r.face_up))
				ent:SetNW2Float("DMS_FaceR", r.face_radius / M)
				-- The head capsule's half length (the build's), for what sits on its surface off the middle.
				DMS_HeadHalf = DMS_HeadHalf or (DMS.BuildParts()[13] or {}).half or 0
				ent:SetNW2Float("DMS_FaceHalf", DMS_HeadHalf / M)
			end
			if r.ring_part and parts and parts[r.ring_part + 1] then
				local p = parts[r.ring_part + 1]
				ent:SetNW2Int("DMS_RingBone", ent:LookupBone(DMS.PartNames()[r.ring_part + 1]) or -1)
				ent:SetNW2Vector("DMS_RingOn", p:GetMassCenter() + V(r.ring_on) / M)
				ent:SetNW2Float("DMS_RingTime", CurTime() - r.ring_age)
			end
		end
	end
end)

local function admin(ply) return not IsValid(ply) or ply:IsSuperAdmin() end

-- The Homer or ValveBiped ragdoll the player looks at (or the only one / all for the server console).
local function aimed(ply)
	if not IsValid(ply) then return nil end
	local tr = ply:GetEyeTrace()
	local ent = tr.Entity
	if IsValid(ent) and (ent:GetModel() == DMS_HOMER_MODEL or DMS_IsValveBiped(ent)) then return ent, tr end
end

local LOOK = "look at a Homer ragdoll (" .. DMS_HOMER_MODEL .. ") or any ValveBiped ragdoll"

-- dms_attach: NM on the ragdoll looked at (balancing): Homer, or any ValveBiped model (ragdolls.lua), as it lies.
concommand.Add("dms_attach", function(ply)
	if not admin(ply) then return end
	local ent = aimed(ply)
	if not ent then return say(ply, "[DMS] dms_attach: " .. LOOK) end
	local c, why = dms.Attach(ent)
	if not c then return say(ply, "[DMS] dms_attach: " .. tostring(why)) end
	local fit = ent.DMS_Native and string.format(" (fitted body: %.2f m, %.1f kg%s)", ent:GetNW2Float("DMS_FitHeight"), ent:GetNW2Float("DMS_FitMass"),
		ent:GetNW2String("DMS_FitWarn") ~= "" and ("; " .. ent:GetNW2String("DMS_FitWarn")) or "") or ""
	say(ply, "[DMS] NM on " .. tostring(ent) .. fit)
end)

-- dms_detach: NM off the ragdoll looked at (an ordinary ragdoll, moving on as it was).
concommand.Add("dms_detach", function(ply)
	if not admin(ply) then return end
	local ent = aimed(ply)
	if not ent then return say(ply, "[DMS] dms_detach: " .. LOOK) end
	if not ent.DMS_Agent then return say(ply, "[DMS] dms_detach: NM isn't on " .. tostring(ent)) end
	dms.Detach(ent)
	say(ply, "[DMS] NM off " .. tostring(ent))
end)

-- dms_detach_all: NM off every body.
concommand.Add("dms_detach_all", function(ply)
	if not admin(ply) then return end
	local n = 0
	for _, ent in pairs(table.Copy(DMS_Agents)) do
		if IsValid(ent) then dms.Detach(ent) n = n + 1 end
	end
	say(ply, string.format("[DMS] NM off %d bodies", n))
end)

-- dms_send <message line>: to the Homer looked at (every Homer from the server console).
concommand.Add("dms_send", function(ply, _, _, line)
	if not admin(ply) then return end
	local targets = IsValid(ply) and { (aimed(ply)) } or DMS_Agents
	for _, ent in pairs(targets) do
		local ok, err = DMS_Send(ent, line)
		if not ok then return say(ply, "[DMS] dms_send: " .. tostring(err)) end
	end
end)

-- A reaction of the Playroom's on `ent` at a point `hit` from `from` (the game's frame, units): without at= or from= in
-- `rest`, they're added as each reaction takes them (shot, blast 1 m from it, taser, melee from `from`, ...).
-- `impulse` (N s) is the blow's, a car's or a thing's push, by default dms_react's. True, or false and why.
function DMS_ReactAt(ent, kind, rest, hit, from, impulse)
	local line = kind .. " " .. (rest or "")
	if not line:find("at=") and not line:find("from=") then
		local d = (hit - from):GetNormalized()
		if kind == "shot" or kind == "taser" then
			line = line .. string.format(" at=%s dir=%s", at(hit), dir(d))
		elseif kind == "blast" then
			line = line .. " at=" .. at(hit - d * 40)
		elseif kind == "melee" then
			line = line .. string.format(" at=%s impulse=%s head=%s", at(hit), at(d * ((impulse or 10) / M)), at(from))
		elseif kind == "nearmiss" then
			line = line .. string.format(" from=%s dir=%s", at(from), dir((hit + Vector(0, 0, 30) - from):GetNormalized()))
		elseif kind == "thrown" then
			line = line .. string.format(" from=%s vel=%s", at(from), at(d * (8 / M)))
		elseif kind == "car" or kind == "prop" then
			line = line .. string.format(" at=%s impulse=%s", at(hit), at(d * ((impulse or 400) / M)))
		end
	end
	return DMS_React(ent, line)
end

-- dms_react <reaction> [key=value ...]: a reaction of the Playroom's on the Homer looked at (every Homer from the
-- server console). Without at= the point looked at (shot, blast 1 m from it, taser, melee from the player).
concommand.Add("dms_react", function(ply, _, args)
	if not admin(ply) then return end
	local kind = args[1]
	if not kind then
		return say(ply, "[DMS] dms_react shot|prop|blast|burn|taser|melee|nearmiss|car|thrown|dropped|button|health|kill "
			.. "[key=value ...] (docs/api.md); buttons: " .. table.concat(DMS.Reactions().buttons, ", "))
	end
	local rest = table.concat(args, " ", 2)
	local ent, tr = aimed(ply)
	local targets = ent and { ent } or (IsValid(ply) and {} or DMS_Agents)
	if not next(targets) then return say(ply, "[DMS] dms_react: look at a Homer with NM on him") end
	for _, e in pairs(targets) do
		local hit = tr and tr.HitPos or e:WorldSpaceCenter()
		local from = IsValid(ply) and ply:EyePos() or (hit + Vector(0, -200, 0))
		local ok, err = DMS_ReactAt(e, kind, rest, hit, from)
		if not ok then return say(ply, "[DMS] dms_react: " .. tostring(err)) end
	end
end)
