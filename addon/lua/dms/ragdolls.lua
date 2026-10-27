-- GMod's ragdolls (ROADMAP M10, docs/adr/0025, 0027): NM on any ValveBiped prop_ragdoll. The model's skeleton is read
-- from its .mdl and Homer's build fitted to it (DMS.RagdollFit, kept by the model and its checksum); the fork makes
-- the fitted body natively in the ragdoll's environment (DMS.RagdollBody, VBOX3D_NM_006), posed and moving as the
-- ragdoll is (or standing in NM's stance), and NM drives it in FOLLOW as it drives Homer. The model's own prop_ragdoll
-- stays the entity the game sees (networked, traced, damaged, undone): its objects are frozen, without collisions and
-- without their own constraints, and put where their bones are on the NM body every tick; the client finishes the 6
-- bones that have no object (cl_ragdolls.lua). Detaching gives the model its physics back, moving as the NM body was:
-- a new ragdoll of the model in its pose (its own constraints can't be put back), the old one gone.
--
-- The physgun and the gravity gun grab NM's body (the Playroom's grab, DMS.FollowGrab) instead of the frozen objects;
-- bullets hit the NM part of the physics bone they hit.

local M = 0.0254
local say = DMS_Say

local PELVIS = "ValveBiped.Bip01_Pelvis"
-- The bones the client finishes, each kept to a physics bone's frame (cl_ragdolls.lua has the same list).
local FINISHED = {
	{ "ValveBiped.Bip01_Spine", "ValveBiped.Bip01_Pelvis" },
	{ "ValveBiped.Bip01_Spine1", "ValveBiped.Bip01_Pelvis" },
	{ "ValveBiped.Bip01_Spine4", "ValveBiped.Bip01_Spine2" },
	{ "ValveBiped.Bip01_Neck1", "ValveBiped.Bip01_Spine2" },
	{ "ValveBiped.Bip01_L_Clavicle", "ValveBiped.Bip01_Spine2" },
	{ "ValveBiped.Bip01_R_Clavicle", "ValveBiped.Bip01_Spine2" },
}

local autoattach = CreateConVar("dms_autoattach_ragdolls", "0", { FCVAR_ARCHIVE, FCVAR_REPLICATED },
	"NM on ValveBiped ragdolls spawned from the spawn menu (they balance where they land)")

local function V(t) return Vector(t.x, t.y, t.z) end
local function A(t) return Angle(t.x, t.y, t.z) end

local function loadData()
	if DMS.DataInfo() then return true end
	return pcall(DMS.LoadData, DMS_GameDir())
end

-- A ValveBiped prop_ragdoll (not Homer's own model).
function DMS_IsValveBiped(ent)
	return IsValid(ent) and ent:GetClass() == "prop_ragdoll" and ent:GetModel() ~= DMS_HOMER_MODEL
		and ent:LookupBone(PELVIS) ~= nil and ent:GetPhysicsObjectCount() > 1
end

-- Homer's build fitted to the model (kept by the module): its fit, or nil and why.
function DMS_RagdollFit(model)
	if not loadData() then return nil, "NM's data isn't loaded" end
	local bytes = file.Read(model, "GAME")
	if not bytes then return nil, model .. ": not found" end
	local ok, f = pcall(DMS.RagdollFit, model, bytes)
	if not ok then return nil, tostring(f) end
	return f
end

-- The ragdoll's physics objects with their bones: {name, obj, pos, index}.
local function objectsOf(ent)
	local out = {}
	for i = 0, ent:GetPhysicsObjectCount() - 1 do
		local p = ent:GetPhysicsObjectNum(i)
		if IsValid(p) then
			out[#out + 1] = { name = ent:GetBoneName(ent:TranslatePhysBoneToBone(i)), obj = p, pos = p:GetPos(), index = i }
		end
	end
	return out
end

-- Where a lying or standing ragdoll faces (degrees): forward from its thighs, as NM's face takes it.
local function facing(objs)
	local l, r
	for _, o in ipairs(objs) do
		if o.name == "ValveBiped.Bip01_L_Thigh" then l = o.pos end
		if o.name == "ValveBiped.Bip01_R_Thigh" then r = o.pos end
	end
	if not l or not r then return 0 end
	local f = Vector(0, 0, 1):Cross(r - l)
	return f:Length() > 0.1 and f:Angle().y or 0
end

-- The model's look while NM drives it: its objects frozen, without collisions and gravity, its own joints gone (or
-- Source's separation fix pulls the posed objects back, ADR 0025).
local function hold(ent)
	ent:RemoveInternalConstraint(-1)
	for i = 0, ent:GetPhysicsObjectCount() - 1 do
		local p = ent:GetPhysicsObjectNum(i)
		if IsValid(p) then
			p:EnableMotion(false)
			p:EnableCollisions(false)
			p:EnableGravity(false)
		end
	end
end

-- NM on a ValveBiped ragdoll: the fitted body takes its pose and every part's velocity (a falling ragdoll keeps
-- falling), or with `stand` ({pos, yaw}: units, degrees) stands in NM's stance there. The agent's id, or nil and why.
function DMS_AttachRagdoll(ent, stand)
	if ent.DMS_Agent then return ent.DMS_Agent end
	if not DMS_IsValveBiped(ent) then return nil, "not a ValveBiped ragdoll" end
	local room, full = DMS_AgentRoom()
	if not room then return nil, full end
	local model = ent:GetModel()
	local f, why = DMS_RagdollFit(model)
	if not f then return nil, why end
	DMS_MapWorld()
	local objs = objectsOf(ent)
	local ok, handle, ms = pcall(DMS.RagdollBody, model, ent:GetPhysicsObjectNum(0), stand or objs)
	if not ok then return nil, "NM's body: " .. tostring(handle) end
	local yaw, floor
	if stand then
		yaw, floor = stand.yaw, stand.pos
	else
		yaw = facing(objs)
		local p = ent:GetPhysicsObjectNum(0):GetPos()
		floor = DMS_FloorBelow(p, ent) or p
	end
	local ok2, id = pcall(DMS.RagdollFollow, model, handle, math.rad(yaw), floor * M)
	if not ok2 then
		DMS.RagdollRemove(handle)
		return nil, "NM: " .. tostring(id)
	end
	hold(ent)
	local bones = {}
	for _, o in ipairs(objs) do bones[o.index] = o.name end
	ent.DMS_Agent, ent.DMS_Native, ent.DMS_Fit, ent.DMS_FitBones = id, handle, model, f.bones
	ent.DMS_PhysBones, ent.DMS_Spawn, ent.DMS_Bound, ent.DMS_NativeMs = bones, { pos = floor, yaw = yaw }, true, ms
	DMS_Agents[id] = ent
	ent:SetNW2Bool("DMS_Ragdoll", true)
	ent:SetNW2Float("DMS_FitHeight", f.height)
	ent:SetNW2Float("DMS_FitMass", f.mass)
	ent:SetNW2String("DMS_FitWarn", table.concat(f.warnings, "; "))
	-- Off (and the NM body gone) before the ragdoll goes.
	ent:CallOnRemove("DMS_Unfollow", function(e) DMS_Unfollow(e) end)
	hook.Run("DMS_Attached", ent, id)
	return id
end

-- A new ragdoll of `ent`'s model as it looks, its objects where `pose` has their bones and moving so (or where
-- `ent`'s are, still), taking `ent`'s place in undo and cleanup. The new ragdoll.
local function rebuild(ent, pose)
	local new = ents.Create("prop_ragdoll")
	new:SetModel(ent:GetModel())
	new:SetPos(ent:GetPos())
	new:SetAngles(ent:GetAngles())
	new:SetSkin(ent:GetSkin())
	for i = 0, ent:GetNumBodyGroups() - 1 do new:SetBodygroup(i, ent:GetBodygroup(i)) end
	new:SetColor(ent:GetColor())
	new:SetRenderMode(ent:GetRenderMode())
	new:SetMaterial(ent:GetMaterial())
	for i = 0, #(ent:GetMaterials() or {}) - 1 do
		local m = ent:GetSubMaterial(i)
		if m ~= "" then new:SetSubMaterial(i, m) end
	end
	if ent.GetCreator and IsValid(ent:GetCreator()) then new:SetCreator(ent:GetCreator()) end
	new:Spawn()
	new:Activate()
	new:SetCollisionGroup(ent:GetCollisionGroup())
	for i = 0, new:GetPhysicsObjectCount() - 1 do
		local p = new:GetPhysicsObjectNum(i)
		local b = pose and pose[new:GetBoneName(new:TranslatePhysBoneToBone(i))]
		local old = ent:GetPhysicsObjectNum(i)
		if IsValid(p) and b then
			local origin = V(b.pos)
			p:SetPos(origin, true)
			p:SetAngles(A(b.ang))
			p:Wake()
			-- The bone's motion carried to the object's centre of mass, its exact frame (DMS.SetBodyState).
			local com = p:LocalToWorld(p:GetMassCenter())
			local w = V(b.spin)
			DMS.SetBodyState(p, com * M, b.rot, V(b.vel) + w:Cross((com - origin) * M), w)
		elseif IsValid(p) and IsValid(old) then
			p:SetPos(old:GetPos(), true)
			p:SetAngles(old:GetAngles())
			p:Wake()
		end
	end
	undo.ReplaceEntity(ent, new)
	cleanup.ReplaceEntity(ent, new)
	return new
end

-- NM off a ragdoll it drives: the model's own physics back, moving as NM's body was (a new ragdoll in its place, the
-- frozen one gone). The new ragdoll.
function DMS_DetachRagdoll(ent)
	if not ent.DMS_Native then return nil end
	local ok, pose = pcall(DMS.PoseModel, ent.DMS_Fit, ent.DMS_Native)
	ent.DMS_Restoring = true
	DMS_Unfollow(ent)
	local new = rebuild(ent, ok and pose or nil)
	ent:Remove()
	return new
end

-- The NM body of a ragdoll goes (follow.lua's forget): the module's bodies, and unless the ragdoll is going too or
-- being given back (DMS_DetachRagdoll), its own physics back where it lies.
function DMS_RagdollGone(ent)
	local handle = ent.DMS_Native
	if not handle then return end
	ent.DMS_Native = nil
	if DMS then DMS.RagdollRemove(handle) end
	ent:SetNW2Bool("DMS_Ragdoll", false)
	-- The bench's (dev.lua) stays as it is till its next run.
	if ent.DMS_Restoring or ent.DMS_Bench or ent:IsMarkedForDeletion() then return end
	-- NM came off by itself (NaN, a limit): not in the middle of its callback.
	timer.Simple(0, function()
		if not IsValid(ent) or ent.DMS_Agent then return end
		rebuild(ent, nil)
		ent:Remove()
	end)
end

-- The NM part a point on the ragdoll is on: its physics bone's, or the nearest to it (a trace along `dir` through it).
function DMS_RagdollPartAt(ent, pos, dir)
	if not ent.DMS_Native then return nil end
	local d = (dir and not dir:IsZero()) and dir:GetNormalized() or Vector(0, 0, -1)
	local tr = util.TraceLine({ start = pos - d * 16, endpos = pos + d * 16, ignoreworld = true, mask = MASK_SHOT })
	if tr.Entity == ent and tr.PhysicsBone then
		local part = (ent.DMS_FitBones or {})[ent.DMS_PhysBones[tr.PhysicsBone] or ""]
		if part then return part end
	end
	return DMS.FollowNearest(ent.DMS_Agent, pos * M)
end

-- Posing (S4): the model's objects where their bones are on NM's body, every tick; the 6 bones without objects for
-- the client, in their physics parent's frame.
hook.Add("Tick", "DMS_Ragdolls", function()
	for _, ent in pairs(DMS_Agents) do
		if IsValid(ent) and ent.DMS_Native then
			local ok, pose = pcall(DMS.PoseModel, ent.DMS_Fit, ent.DMS_Native)
			if ok then
				for i, name in pairs(ent.DMS_PhysBones) do
					local b, p = pose[name], ent:GetPhysicsObjectNum(i)
					if b and IsValid(p) then
						p:SetPos(V(b.pos), true)
						p:SetAngles(A(b.ang))
					end
				end
				for k, f in ipairs(FINISHED) do
					local b, par = pose[f[1]], pose[f[2]]
					if b and par then
						local lp, la = WorldToLocal(V(b.pos), A(b.ang), V(par.pos), A(par.ang))
						ent:SetNW2Vector("DMS_FP" .. k, lp)
						ent:SetNW2Angle("DMS_FA" .. k, la)
					end
				end
			end
		end
	end
end)

-- The physgun and the gravity gun on a ragdoll NM drives grab NM's body: the Playroom's grab (a spring on the point
-- held toward where the gun holds it), not the model's frozen objects.
local grabs = {} -- player -> {ent, kind, dist}
local GRAV_REACH, GRAV_HOLD = 250, 70

local function release(ply)
	local g = grabs[ply]
	grabs[ply] = nil
	if not g or not IsValid(g.ent) or not g.ent.DMS_Agent then return end
	DMS.FollowGrab(g.ent.DMS_Agent)
	local s = DMS.FollowState(g.ent.DMS_Agent)
	if s and Vector(s.com_v.x, s.com_v.y, s.com_v.z):Length() > 1 then DMS_React(g.ent, "dropped") end
end

local function aimAt(ply, reach)
	local tr = util.TraceLine({ start = ply:EyePos(), endpos = ply:EyePos() + ply:GetAimVector() * reach, filter = ply, mask = MASK_SHOT })
	if IsValid(tr.Entity) and tr.Entity.DMS_Native then return tr end
end

local function grab(ply, tr, kind, dist)
	local ent = tr.Entity
	local part = DMS_RagdollPartAt(ent, tr.HitPos, ply:GetAimVector()) or 0
	grabs[ply] = { ent = ent, kind = kind, dist = dist }
	DMS_Wake(ent)
	DMS.FollowGrab(ent.DMS_Agent, (ply:EyePos() + ply:GetAimVector() * dist) * M, part, tr.HitPos * M)
end

hook.Add("PhysgunPickup", "DMS_Ragdolls", function(_, ent) if ent.DMS_Native then return false end end)
hook.Add("GravGunPickupAllowed", "DMS_Ragdolls", function(_, ent) if ent.DMS_Native then return false end end)
hook.Add("GravGunPunt", "DMS_Ragdolls", function(_, ent) if ent.DMS_Native then return false end end)

local function weapon(ply)
	local w = ply:GetActiveWeapon()
	return IsValid(w) and w:GetClass()
end

hook.Add("KeyPress", "DMS_Ragdolls", function(ply, key)
	local w, g = weapon(ply), grabs[ply]
	if w == "weapon_physgun" and key == IN_ATTACK and not g then
		local tr = aimAt(ply, 4096)
		if tr then grab(ply, tr, "physgun", tr.HitPos:Distance(ply:EyePos())) end
	elseif w == "weapon_physcannon" and key == IN_ATTACK2 then
		if g then return release(ply) end
		local tr = aimAt(ply, GRAV_REACH)
		if tr then grab(ply, tr, "gravgun", GRAV_HOLD) end
	elseif w == "weapon_physcannon" and key == IN_ATTACK then
		local ent = g and g.ent
		local at
		if g then
			release(ply)
			local s = IsValid(ent) and DMS.FollowState(ent.DMS_Agent)
			at = s and Vector(s.com.x, s.com.y, s.com.z) / M
		else
			local tr = aimAt(ply, GRAV_REACH)
			ent, at = tr and tr.Entity, tr and tr.HitPos
		end
		-- The punt: the Playroom's poke along the aim.
		if IsValid(ent) and at then
			local d = ply:GetAimVector()
			DMS_React(ent, string.format("poke at=%.4f,%.4f,%.4f dir=%.4f,%.4f,%.4f", at.x * M, at.y * M, at.z * M, d.x, d.y, d.z))
		end
	end
end)

hook.Add("KeyRelease", "DMS_Ragdolls", function(ply, key)
	local g = grabs[ply]
	if g and g.kind == "physgun" and key == IN_ATTACK then release(ply) end
end)

hook.Add("Think", "DMS_RagdollGrab", function()
	for ply, g in pairs(grabs) do
		local w = IsValid(ply) and weapon(ply)
		if not IsValid(g.ent) or not g.ent.DMS_Agent or w ~= (g.kind == "physgun" and "weapon_physgun" or "weapon_physcannon") then
			if IsValid(ply) then release(ply) else grabs[ply] = nil end
		else
			DMS.FollowGrab(g.ent.DMS_Agent, (ply:EyePos() + ply:GetAimVector() * g.dist) * M)
		end
	end
end)

-- Ragdolls spawned from the spawn menu, with dms_autoattach_ragdolls.
hook.Add("PlayerSpawnedRagdoll", "DMS_Ragdolls", function(ply, _, ent)
	if not autoattach:GetBool() or not DMS_IsValveBiped(ent) then return end
	timer.Simple(0, function()
		if not IsValid(ent) then return end
		local id, why = DMS_AttachRagdoll(ent)
		if id then DMS_Send(ent, "bodyBalance start=true") else DMS_Say(ply, "[DMS] NM isn't on the ragdoll: " .. tostring(why)) end
	end)
end)

-- A model's ragdoll standing in NM's stance at pos (its feet's level) facing yaw (degrees), NM on it. The entity, or
-- nil and why.
function DMS_SpawnRagdoll(model, pos, yaw)
	if not util.IsValidModel(model) then return nil, model .. ": no such model" end
	local ent = ents.Create("prop_ragdoll")
	ent:SetModel(model)
	ent:SetPos(pos)
	ent:SetAngles(Angle(0, yaw, 0))
	ent:Spawn()
	ent:Activate()
	local id, why = DMS_AttachRagdoll(ent, { pos = pos, yaw = yaw })
	if not id then
		ent:Remove()
		return nil, why
	end
	return ent
end

local function admin(ply) return not IsValid(ply) or ply:IsSuperAdmin() end

-- dms_ragdoll [model]: a model's ragdoll in front of the player, standing, NM on it balancing (superadmins).
concommand.Add("dms_ragdoll", function(ply, _, args)
	if not admin(ply) then return end
	local model = args[1] or "models/humans/group01/male_07.mdl"
	local pos, yaw = Vector(0, 0, 0), 0
	if IsValid(ply) then
		local tr = util.TraceLine({ start = ply:EyePos(), endpos = ply:EyePos() + ply:GetAimVector() * 150, filter = ply })
		pos, yaw = DMS_FloorAt(tr.HitPos, ply), ply:EyeAngles().y + 180
	end
	local ent, why = DMS_SpawnRagdoll(model, pos, yaw)
	if not ent then return say(ply, "[DMS] dms_ragdoll: " .. tostring(why)) end
	if IsValid(ply) then
		undo.Create("DMS ragdoll")
		undo.AddEntity(ent)
		undo.SetPlayer(ply)
		undo.Finish()
		ply:AddCleanup("ragdolls", ent)
	end
	DMS_Send(ent, "bodyBalance start=true")
	say(ply, string.format("[DMS] %s: %.2f m, %.1f kg, NM agent #%d, its body made in %.1f ms%s", model, ent:GetNW2Float("DMS_FitHeight"),
		ent:GetNW2Float("DMS_FitMass"), ent.DMS_Agent, ent.DMS_NativeMs or 0,
		ent:GetNW2String("DMS_FitWarn") ~= "" and (" (" .. ent:GetNW2String("DMS_FitWarn") .. ")") or ""))
end)

-- dms_fit <model>: the model's fit (sizes, mass, warnings), without making anything.
concommand.Add("dms_fit", function(ply, _, args)
	if not admin(ply) then return end
	local model = args[1] or "models/humans/group01/male_07.mdl"
	local f, why = DMS_RagdollFit(model)
	if not f then return say(ply, "[DMS] dms_fit: " .. tostring(why)) end
	say(ply, string.format("[DMS] %s: %.3f m, %.1f kg (Homer %.3f m, %.1f kg), %s in %.2f ms%s", model, f.height, f.mass, f.homer_height,
		f.homer_mass, f.cached and "cached" or "fitted", f.ms, #f.warnings > 0 and ("\n  " .. table.concat(f.warnings, "\n  ")) or ""))
end)
