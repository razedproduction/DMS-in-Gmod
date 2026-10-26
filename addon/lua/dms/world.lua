-- NM's world (ROADMAP M5, docs/adr/0018): the map's static geometry from its .bsp (module/src/world.rs), the
-- surface of each material for NM's friction, and the props near each FOLLOW agent as NM's boxes every tick.

local METRES_PER_UNIT = 0.0254
-- The fork's shell around every hull (vphysics conversion.h SHAPE_INFLATION, units): Box3D's floor is that much over
-- the BSP's plane, and so is everything a trace hits.
DMS_SHELL = 0.25 - 0.0001
-- How far around an agent props become NM's obstacles (units).
local OBSTACLE_REACH = 3 / METRES_PER_UNIT

local say = DMS_Say

-- A trace stops this short of the plane it hits (Source's DIST_EPSILON, units).
local TRACE_GAP = 1 / 32

-- Where Box3D's floor is under pos (units): the plane the trace hit moved out by the shell, and its normal; nil
-- for none.
function DMS_FloorBelow(pos, filter)
	local tr = util.TraceLine({ start = pos + Vector(0, 0, 64), endpos = pos - Vector(0, 0, 32768),
		mask = MASK_SOLID_BRUSHONLY, filter = filter })
	if tr.Hit then return tr.HitPos + tr.HitNormal * (DMS_SHELL - TRACE_GAP), tr.HitNormal end
end

local loaded -- the map whose world the module has, with its surfaces named
local failed -- why it couldn't be read

-- The map's world for NM, once a map (it needs NM's data for the surfaces). True, or false and why (agents then
-- stand on a traced floor).
function DMS_MapWorld()
	local map = game.GetMap()
	if loaded == map then return true end
	if failed and failed.map == map then return false, failed.why end
	if not DMS.DataInfo() then return false, "NM's data isn't loaded" end
	local ok, info = pcall(DMS.MapLoad, map)
	if not ok then
		local bytes = file.Read("maps/" .. map .. ".bsp", "GAME")
		if not bytes then
			ok, info = false, "maps/" .. map .. ".bsp isn't readable"
		else
			ok, info = pcall(DMS.MapLoad, map, bytes)
		end
	end
	if not ok then
		DMS.MapUnload()
		failed = { map = map, why = tostring(info) }
		print("[DMS] NM's world: " .. failed.why .. " (agents stand on a traced floor)")
		return false, failed.why
	end
	local props = {}
	for i, name in ipairs(info.materials) do
		local m = Material(name)
		props[i] = (m and not m:IsError() and m:GetString("$surfaceprop")) or ""
	end
	local rows = DMS.MapSurfaces(props)
	local used = {}
	for i, r in ipairs(rows) do used[r] = (used[r] or 0) + 1 end
	local list = {}
	for r, n in SortedPairs(used) do list[#list + 1] = r .. " " .. n end
	print(string.format("[DMS] NM's world: %s, %d faces, %d displacements (%d polygons); surfaces: %s", map, info.faces,
		info.displacements, info.polygons, table.concat(list, ", ")))
	loaded = map
	return true
end

hook.Add("InitPostEntity", "DMS_World", function()
	loaded, failed = nil, nil
	DMS_MapWorld()
end)

local function admin(ply) return not IsValid(ply) or ply:IsSuperAdmin() end

-- dms_map: NM's world of this map (read again with "reload").
concommand.Add("dms_map", function(ply, _, args)
	if not admin(ply) then return end
	if args[1] == "reload" then loaded, failed = nil, nil end
	local ok, why = DMS_MapWorld()
	say(ply, ok and ("[DMS] NM's world: " .. game.GetMap()) or ("[DMS] NM's world: none: " .. tostring(why)))
end)

-- NM's world against the game's at points around pos (units): heights of NM's ground and of a trace (+ shell).
function DMS_MapCheck(pos, half, n)
	DMS_MapWorld()
	local worst, out = 0, {}
	for i = 0, n - 1 do
		for j = 0, n - 1 do
			local p = pos + Vector((i / math.max(n - 1, 1) * 2 - 1) * half, (j / math.max(n - 1, 1) * 2 - 1) * half, 0)
			local floor = DMS_FloorBelow(p)
			local z, row = DMS.MapGround(p * METRES_PER_UNIT + Vector(0, 0, 64 * METRES_PER_UNIT), 400)
			if floor and z then
				local d = z - floor.z * METRES_PER_UNIT
				worst = math.max(worst, math.abs(d))
				out[#out + 1] = string.format("%.0f %.0f: %+.4f m %s", p.x, p.y, d, row)
			else
				out[#out + 1] = string.format("%.0f %.0f: trace %s, NM %s", p.x, p.y, tostring(floor), tostring(z))
				worst = math.huge
			end
		end
	end
	return worst, out
end

-- The props near an agent, as `DMS.FollowObstacles` takes them.
local function obstaclesNear(ent, centre)
	local list = {}
	for _, e in ipairs(ents.FindInSphere(centre, OBSTACLE_REACH)) do
		local phys = e:GetPhysicsObject()
		local class = e:GetClass()
		if e ~= ent and IsValid(phys) and e:GetMoveType() == MOVETYPE_VPHYSICS and class ~= "prop_ragdoll"
			and not e:IsPlayer() and not e:IsNPC() then
			local lo, hi = e:OBBMins(), e:OBBMaxs()
			local c = e:LocalToWorld((lo + hi) * 0.5) * METRES_PER_UNIT
			local f, l, u = e:GetForward(), -e:GetRight(), e:GetUp()
			local half = (hi - lo) * (0.5 * METRES_PER_UNIT)
			local v = phys:GetVelocity() * METRES_PER_UNIT
			local w = phys:LocalToWorldVector(phys:GetAngleVelocity()) * (math.pi / 180)
			local com = phys:LocalToWorld(phys:GetMassCenter()) * METRES_PER_UNIT
			for _, x in ipairs({ e:EntIndex(), c.x, c.y, c.z, f.x, f.y, f.z, l.x, l.y, l.z, u.x, u.y, u.z, half.x, half.y,
				half.z, v.x, v.y, v.z, w.x, w.y, w.z, com.x, com.y, com.z }) do
				list[#list + 1] = x
			end
		end
	end
	return list
end

hook.Add("Tick", "DMS_Obstacles", function()
	for id, ent in pairs(DMS_Agents or {}) do
		local s = IsValid(ent) and DMS.FollowState(id)
		if s then DMS.FollowObstacles(id, obstaclesNear(ent, Vector(s.com.x, s.com.y, s.com.z) / METRES_PER_UNIT)) end
	end
end)
