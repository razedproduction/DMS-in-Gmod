-- The bench's scene (ROADMAP M2.5): the Playroom's room on Source. For now its flat floor and spawn point only —
-- the Playroom's stages (stairs, wall, ledge, bridge) come with NM's world from the map (M5).
--
-- On a flat map (gm_flatgrass) the room's origin is the floor under the map's player start. DMS.BenchScene()
-- checks the floor is flat over the Playroom's room and says where the origin is; Homer's FOLLOW agent (M4) spawns
-- there, facing as in the Playroom (yaw 0: the build's pose).

local METRES_PER_UNIT = 0.0254
-- How far around the origin the floor has to be flat (m), and how flat (units).
local ROOM_HALF = 5
local FLAT = 0.5

local function floorBelow(pos)
	local tr = util.TraceLine({ start = pos + Vector(0, 0, 64), endpos = pos - Vector(0, 0, 32768), mask = MASK_SOLID_BRUSHONLY })
	if tr.Hit then return tr.HitPos, tr.HitNormal end
end

-- { map, origin = Vector (units), flat = bool, worst = units, normal = Vector } or nil, error.
function DMS.BenchScene()
	local start = ents.FindByClass("info_player_start")[1]
	local from = IsValid(start) and start:GetPos() or Vector(0, 0, 0)
	local origin, normal = floorBelow(from)
	if not origin then return nil, "no floor under the player start" end
	local half, worst = ROOM_HALF / METRES_PER_UNIT, 0
	for x = -1, 1, 0.5 do
		for y = -1, 1, 0.5 do
			local hit = floorBelow(origin + Vector(x * half, y * half, 0))
			worst = math.max(worst, hit and math.abs(hit.z - origin.z) or math.huge)
		end
	end
	return { map = game.GetMap(), origin = origin, normal = normal, flat = worst <= FLAT, worst = worst }
end

concommand.Add("dms_bench_scene", function(ply)
	if IsValid(ply) and not ply:IsSuperAdmin() then return end
	local s, err = DMS.BenchScene()
	if not s then return print("[DMS] bench scene: " .. err) end
	print(string.format("[DMS] bench scene: %s, the room's origin at (%.1f, %.1f, %.1f) units; floor %s over +-%d m " ..
		"(worst %.2f units); stages: M5", s.map, s.origin.x, s.origin.y, s.origin.z, s.flat and "flat" or "NOT flat",
		ROOM_HALF, s.worst))
end)
