-- The bench's scene (ROADMAP M2.5, M5): the Playroom's room on Source.
--
-- The room's origin is Box3D's floor (DMS_FloorBelow: the BSP's plane plus the fork's shell) under the map's player
-- start; a Playroom stage's (`--stage`, ROADMAP M5) is under its `info_target` `dms_stage_<name>` on the stages map
-- (tools/dms-map, docs/bench.md). DMS.BenchScene() checks the floor is flat over the Playroom's room and says where
-- the origin is; Homer's FOLLOW agent spawns there, facing as in the Playroom (yaw 0: the build's pose). NM's world
-- is the map's (world.lua).

local METRES_PER_UNIT = 0.0254
-- How far around the origin the floor has to be flat (m), and how flat (units).
local ROOM_HALF = 5
local FLAT = 0.5

-- { map, stage, origin = Vector (units), flat = bool, worst = units, normal = Vector } or nil, error.
function DMS.BenchScene(stage)
	local from
	if stage and stage ~= "" and stage ~= "room" then
		local t = ents.FindByName("dms_stage_" .. stage)[1]
		if not IsValid(t) then
			return nil, "no stage " .. stage .. " on " .. game.GetMap() .. " (tools/dms-map makes dms_stages)"
		end
		from = t:GetPos()
	else
		local start = ents.FindByClass("info_player_start")[1]
		from = IsValid(start) and start:GetPos() or Vector(0, 0, 0)
	end
	-- A stage's start is inside its boxes (the stairs, the roof): its floor's height is taken 20 m to its side.
	local clear = stage and stage ~= "" and stage ~= "room" and Vector(0, -20 / METRES_PER_UNIT, 0) or Vector(0, 0, 0)
	local origin, normal = DMS_FloorBelow(from + clear)
	if not origin then return nil, "no floor under the scene's start" end
	origin = Vector(from.x, from.y, origin.z)
	local half, worst = ROOM_HALF / METRES_PER_UNIT, 0
	for x = -1, 1, 0.5 do
		for y = -1, 1, 0.5 do
			local hit = DMS_FloorBelow(origin + Vector(x * half, y * half, 0))
			worst = math.max(worst, hit and math.abs(hit.z - origin.z) or math.huge)
		end
	end
	return { map = game.GetMap(), stage = stage or "room", origin = origin, normal = normal, flat = worst <= FLAT, worst = worst }
end

concommand.Add("dms_bench_scene", function(ply, _, args)
	if IsValid(ply) and not ply:IsSuperAdmin() then return end
	local s, err = DMS.BenchScene(args[1])
	if not s then return print("[DMS] bench scene: " .. err) end
	print(string.format("[DMS] bench scene: %s, stage %s, the room's origin at (%.2f, %.2f, %.2f) units; floor %s over " ..
		"+-%d m (worst %.2f units)", s.map, s.stage, s.origin.x, s.origin.y, s.origin.z, s.flat and "flat" or "NOT flat",
		ROOM_HALF, s.worst))
end)
