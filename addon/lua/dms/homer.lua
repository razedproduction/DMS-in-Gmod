-- Homer's ragdoll (ROADMAP M3): models/dms/homer.mdl, made by tools/dms-homer from the user's GTA IV, bound to NM's
-- body (DMS.BindHomer: NM's pivots, frames and limits on every joint) right as it's spawned, in his standing pose.

local METRES_PER_UNIT = 0.0254
DMS_HOMER_MODEL = "models/dms/homer.mdl"

local say = DMS_Say

-- The ragdoll's PhysObj in NM's part order (its bones are named as NM's parts), or nil and why.
function DMS_HomerParts(ent)
	local byName = {}
	for i = 0, ent:GetPhysicsObjectCount() - 1 do
		local name = ent:GetBoneName(ent:TranslatePhysBoneToBone(i))
		byName[name] = ent:GetPhysicsObjectNum(i)
	end
	local parts = {}
	for i, name in ipairs(DMS.PartNames()) do
		local phys = byName[name]
		if not IsValid(phys) then return nil, "no physics object for " .. name end
		parts[i] = phys
	end
	return parts
end

-- Spawns Homer standing at pos (his feet's level) facing yaw (degrees) and binds him to NM's body. The entity, or
-- nil and why.
function DMS_SpawnHomer(pos, yaw)
	if not util.IsValidModel(DMS_HOMER_MODEL) then
		return nil, DMS_HOMER_MODEL .. " isn't there: make it with tools\\dms-homer from your GTA IV (docs/homer.md)"
	end
	if not DMS.DataInfo() then
		local ok, err = pcall(DMS.LoadData, GetConVar("dms_game_dir"):GetString())
		if not ok then return nil, "NM's data isn't loaded: " .. tostring(err) end
	end
	-- NM's world and friction (world.lua) before NM's materials go on his parts.
	DMS_MapWorld()
	local ent = ents.Create("prop_ragdoll")
	ent:SetModel(DMS_HOMER_MODEL)
	ent:SetPos(pos)
	ent:SetAngles(Angle(0, yaw, 0))
	ent:Spawn()
	ent:Activate()
	local parts, why = DMS_HomerParts(ent)
	if not parts then
		ent:Remove()
		return nil, why
	end
	local ok, err = pcall(DMS.BindHomer, parts)
	if not ok then
		ent:Remove()
		return nil, "bind: " .. tostring(err)
	end
	ent.DMS_Spawn = { pos = pos, yaw = yaw }
	return ent
end

-- Homer's parts as Box3D has them, a line each (for tools/golden body): the spawn's `spawn x y z yaw` (m, degrees),
-- then `part <name> <state pos 3> <state rot 4> <origin 3> <mass> <inertia 3> <damping 2> <tensor 9>` (SI; origin
-- the PhysObj's own, inertia GetInertia's diagonal, tensor the one Box3D turns the body with, DMS.BodyInertia).
function DMS_HomerReport(ent)
	local parts, why = DMS_HomerParts(ent)
	if not parts then return "error " .. why end
	local s = ent.DMS_Spawn or { pos = ent:GetPos(), yaw = ent:GetAngles().y }
	local out = { string.format("spawn %.9g %.9g %.9g %.9g", s.pos.x * METRES_PER_UNIT, s.pos.y * METRES_PER_UNIT,
		s.pos.z * METRES_PER_UNIT, s.yaw) }
	for i, phys in ipairs(parts) do
		local st = DMS.BodyState(phys)
		local o = phys:GetPos() * METRES_PER_UNIT
		local inertia = phys:GetInertia()
		local lin, ang = phys:GetDamping()
		out[#out + 1] = string.format("part %s %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g",
			DMS.PartNames()[i], st.pos.x, st.pos.y, st.pos.z, st.rot.x, st.rot.y, st.rot.z, st.rot.w, o.x, o.y, o.z,
			phys:GetMass(), inertia.x, inertia.y, inertia.z, lin, ang)
		local t = DMS.BodyInertia(phys)
		if t then
			out[#out] = out[#out] .. string.format(" %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g %.9g", unpack(t))
		end
	end
	return table.concat(out, "\n")
end

-- dms_homer: Homer standing in front of the player, facing him, NM on him (superadmins).
concommand.Add("dms_homer", function(ply)
	if IsValid(ply) and not ply:IsSuperAdmin() then return end
	local pos, yaw
	if IsValid(ply) then
		local tr = util.TraceLine({ start = ply:EyePos(), endpos = ply:EyePos() + ply:GetAimVector() * 150,
			filter = ply })
		-- His feet on Box3D's floor: the trace's plane moved out by the fork's shell (docs/adr/0018).
		local down = util.TraceLine({ start = tr.HitPos + Vector(0, 0, 8), endpos = tr.HitPos - Vector(0, 0, 400),
			filter = ply })
		pos = down.Hit and (DMS_FloorBelow(down.HitPos + Vector(0, 0, 1), ply) or down.HitPos) or down.HitPos
		yaw = ply:EyeAngles().y + 180
	else
		pos, yaw = Vector(0, 0, 0), 0
	end
	local ent, why = DMS_SpawnHomer(pos, yaw)
	if not ent then return say(ply, "[DMS] dms_homer: " .. why) end
	if IsValid(ply) then
		undo.Create("DMS Homer")
		undo.AddEntity(ent)
		undo.SetPlayer(ply)
		undo.Finish()
		ply:AddCleanup("ragdolls", ent)
	end
	-- ROADMAP M4: NM drives him (FOLLOW), standing under bodyBalance as the Playroom's "balance" button has it.
	local id, err = DMS_Follow(ent)
	if not id then return say(ply, "[DMS] dms_homer: NM isn't on him: " .. tostring(err)) end
	DMS_Send(ent, "bodyBalance start=true")
	say(ply, string.format("[DMS] Homer at (%.1f, %.1f, %.1f), NM agent #%d, bodyBalance", pos.x, pos.y, pos.z, id))
end)
