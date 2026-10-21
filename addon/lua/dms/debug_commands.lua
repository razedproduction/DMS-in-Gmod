-- Debug console commands over the IPhysicsNM natives (ROADMAP M1). Superadmins only, on what they aim at.

local METRES_PER_UNIT = 0.0254

-- Hovering props: entity -> its PhysObj, so DMS.Hover can be stopped before the entity goes.
local hovering = {}

local function say(ply, text)
	if IsValid(ply) then
		ply:PrintMessage(HUD_PRINTCONSOLE, text)
	else
		print(text)
	end
end

local function allowed(ply)
	if IsValid(ply) and not ply:IsSuperAdmin() then return false end
	if not DMS.Available() then
		say(ply, "[DMS] IPhysicsNM isn't available, see the server console")
		return false
	end
	return true
end

-- The PhysObj (the hit bone's one on a ragdoll) of what the player aims at.
local function aimed(ply)
	if not IsValid(ply) then return end
	local tr = ply:GetEyeTrace()
	local ent = tr.Entity
	if not IsValid(ent) then return end
	local phys = ent:GetPhysicsObjectNum(tr.PhysicsBone or 0)
	if not IsValid(phys) then phys = ent:GetPhysicsObject() end
	if not IsValid(phys) then return end
	return ent, phys, tr.PhysicsBone or 0
end

local function fmt(v)
	return string.format("(%.4f, %.4f, %.4f)", v.x, v.y, v.z)
end

local function diff(a, b)
	return math.sqrt((a.x - b.x) ^ 2 + (a.y - b.y) ^ 2 + (a.z - b.z) ^ 2)
end

local function toSI(v)
	return { x = v.x * METRES_PER_UNIT, y = v.y * METRES_PER_UNIT, z = v.z * METRES_PER_UNIT }
end

-- The body's X axis in world from the state's quaternion (x, y, z, w).
local function quatAxisX(q)
	return {
		x = 1 - 2 * (q.y * q.y + q.z * q.z),
		y = 2 * (q.x * q.y + q.w * q.z),
		z = 2 * (q.x * q.z - q.w * q.y),
	}
end

concommand.Add("dms_info", function(ply)
	if IsValid(ply) and not ply:IsSuperAdmin() then return end
	say(ply, string.format("[DMS] module %s, SDK %d, IPhysicsNM: %s, sub-steps: %s", DMS.Version, DMS.NmVersion(),
		DMS.InterfaceVersion() or "none", DMS.Available() and DMS.SubstepCount() or "-"))
end)

-- BodyState of the aimed part next to the same values from GMod's PhysObj, in SI.
concommand.Add("dms_bodystate", function(ply)
	if not allowed(ply) then return end
	local ent, phys, bone = aimed(ply)
	if not ent then return say(ply, "[DMS] aim at a physics object") end
	local s = DMS.BodyState(phys)
	if not s then return say(ply, "[DMS] BodyState failed") end

	local com = toSI(phys:LocalToWorld(phys:GetMassCenter()))
	local vel = toSI(phys:GetVelocity())
	local angvel = phys:LocalToWorldVector(phys:GetAngleVelocity()) * math.pi / 180
	local axis = phys:LocalToWorldVector(Vector(1, 0, 0))
	local qaxis = quatAxisX(s.rot)

	say(ply, string.format("[DMS] %s, PhysObj %d, mass %.3f kg", tostring(ent), bone,
		phys:GetMass()))
	say(ply, "  pos    NM " .. fmt(s.pos) .. "  GMod COM " .. fmt(com) .. string.format("  |d| %.5f m", diff(s.pos, com)))
	say(ply, "  vel    NM " .. fmt(s.vel) .. "  GMod " .. fmt(vel) .. string.format("  |d| %.5f m/s", diff(s.vel, vel)))
	say(ply, "  angvel NM " .. fmt(s.angvel) .. "  GMod " .. fmt(angvel) ..
		string.format("  |d| %.5f rad/s", diff(s.angvel, angvel)))
	say(ply, string.format("  rot    NM (%.4f, %.4f, %.4f, %.4f)  X axis NM %s  GMod %s  |d| %.5f", s.rot.x, s.rot.y,
		s.rot.z, s.rot.w, fmt(qaxis), fmt(axis), diff(qaxis, axis)))
end)

-- Holds the aimed object up with m*g every physics step, or lets it go.
concommand.Add("dms_hover", function(ply)
	if not allowed(ply) then return end
	local ent, phys = aimed(ply)
	if not ent then return say(ply, "[DMS] aim at a physics object") end

	if hovering[ent] then
		DMS.Hover(hovering[ent], false)
		hovering[ent] = nil
		return say(ply, "[DMS] hover off: " .. tostring(ent))
	end
	local g = physenv.GetGravity():Length() * METRES_PER_UNIT
	DMS.Hover(phys, phys:GetMass(), g)
	hovering[ent] = phys
	say(ply, string.format("[DMS] hover on: %s, %.3f kg * %.3f m/s^2 = %.3f N up", tostring(ent), phys:GetMass(), g,
		phys:GetMass() * g))
end)

hook.Add("EntityRemoved", "DMS_Hover", function(ent)
	local phys = hovering[ent]
	if not phys then return end
	hovering[ent] = nil
	DMS.Hover(phys, false)
end)

hook.Add("ShutDown", "DMS_Hover", function()
	hovering = {}
end)

-- Sets the armature (kg m^2) of every joint of the aimed ragdoll: pairs of its parts FindJoint joins.
concommand.Add("dms_armature", function(ply, _, args)
	if not allowed(ply) then return end
	local value = tonumber(args[1])
	if not value or value < 0 then return say(ply, "[DMS] usage: dms_armature <kg m^2, 0 to 1e6>") end
	local ent = aimed(ply)
	if not ent then return say(ply, "[DMS] aim at a ragdoll") end

	local n = ent:GetPhysicsObjectCount()
	local joints, set = 0, 0
	for i = 0, n - 1 do
		local a = ent:GetPhysicsObjectNum(i)
		for j = i + 1, n - 1 do
			local b = ent:GetPhysicsObjectNum(j)
			if IsValid(a) and IsValid(b) and DMS.FindJoint(a, b) then
				joints = joints + 1
				if DMS.SetArmature(a, b, value) then set = set + 1 end
				say(ply, string.format("  %d-%d: armature %g", i, j, DMS.GetArmature(a, b) or -1))
			end
		end
	end
	say(ply, string.format("[DMS] %s: %d parts, %d joints, armature %g set on %d", tostring(ent), n, joints, value, set))
end)
