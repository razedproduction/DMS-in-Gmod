-- Homer's face as the Playroom draws it (`delirium-room` `face`, ROADMAP M6): eye whites, pupils and brows on his head
-- capsule. The eyes blink, look at where he was last hit (or along his way when he's quick), squint with pain, shut
-- when he's dead or blinking; the brows show when he's scared, worried or hurt. The server sends NM's view (NW2 vars,
-- reactions.lua); the head is the bone's, so the face moves with the drawn ragdoll.

local eyes = CreateClientConVar("dms_eyes", "1", true, false, "Homer's eyes (the Playroom's)")
local brows = CreateClientConVar("dms_brows", "1", true, false, "Homer's eyebrows (the Playroom's)")

local SPHERE = "models/hunter/misc/sphere025x025.mdl"
local WHITE = Material("models/debug/debugwhite")
local model

local function piece(pos, x, y, z, sx, sy, sz, r, g, b)
	if not IsValid(model) then
		model = ClientsideModel(SPHERE, RENDERGROUP_OPAQUE)
		if not IsValid(model) then return end
		model:SetNoDraw(true)
	end
	local radius = model:GetModelRadius()
	if radius <= 0 or sx <= 0 then return end
	local m = Matrix({
		{ x.x * sx, y.x * sy, z.x * sz, 0 },
		{ x.y * sx, y.y * sy, z.y * sz, 0 },
		{ x.z * sx, y.z * sy, z.z * sz, 0 },
		{ 0, 0, 0, 1 },
	})
	m:Scale(Vector(1, 1, 1) / radius)
	model:SetPos(pos)
	model:SetAngles(angle_zero)
	model:EnableMatrix("RenderMultiply", m)
	model:SetupBones()
	render.SetColorModulation(r, g, b)
	model:DrawModel()
end

local function draw(ent)
	local bone = ent:GetNW2Int("DMS_HeadBone", -1)
	local r = ent:GetNW2Float("DMS_FaceR", 0)
	if bone < 0 or r <= 0 then return end
	local bp, ba = ent:GetBonePosition(bone)
	if not bp then return end
	local centre = LocalToWorld(ent:GetNW2Vector("DMS_FaceCentre"), angle_zero, bp, ba)
	local rot = function(v) return LocalToWorld(v, angle_zero, vector_origin, ba) end
	local fwd, up = rot(ent:GetNW2Vector("DMS_FaceFwd")), rot(ent:GetNW2Vector("DMS_FaceUp"))
	local left = up:Cross(fwd):GetNormalized()
	local v = ent:GetNW2Vector("DMS_V") -- m/s
	local speed = v:Length()
	local fear = math.Clamp((speed - 1) / 2 + math.max(-v.z - 0.5, 0) * 0.6, 0, 1)
	-- The last hit (the Playroom's ring), 1.5 s of pain.
	local age = CurTime() - ent:GetNW2Float("DMS_RingTime", -100)
	local hitAt
	local rb = ent:GetNW2Int("DMS_RingBone", -1)
	if rb >= 0 and age < 1.5 then
		local p, a = ent:GetBonePosition(rb)
		if p then hitAt = LocalToWorld(ent:GetNW2Vector("DMS_RingOn"), angle_zero, p, a) end
	end
	local pain = hitAt and math.Clamp(1 - age / 1.5, 0, 1) or 0
	local worry = math.Clamp((speed - 0.3) / 0.7, 0, 1) * (1 - fear)
	-- Lying with his head under 0.5 m (NM's frame starts on the floor he was put on), still.
	local dazed = (centre.z - ent:GetNW2Float("DMS_Floor", centre.z - 100) < 0.5 / 0.0254 and speed < 0.3) and 1 or 0
	local upset = math.max(fear, worry, pain)
	local t = CurTime() + ent:EntIndex() * 1.7
	local blink = (t + 0.7 * math.sin(math.floor(t / 4))) % 4 < 0.12 or (hitAt and age < 0.3)
	local dead = ent:GetNW2Bool("DMS_Dead")
	local open = (dead or blink) and 0.08 or (1 + 0.3 * fear) * (1 - 0.8 * math.max(pain, 0.6 * dazed))
	local look = hitAt and (hitAt - centre):GetNormalized() or (speed > 1.5 and v / speed) or fwd
	local gaze = look - fwd * look:Dot(fwd)
	if gaze:Length() > 1 then gaze:Normalize() end
	gaze = gaze * 0.12
	render.SetMaterial(WHITE)
	render.MaterialOverride(WHITE)
	for i = 0, 5 do
		local side = i % 2 == 0 and 1 or -1
		local n = (fwd + up * 0.1 + left * side * 0.3 + gaze):GetNormalized()
		local el = up:Cross(n):GetNormalized()
		local ey = n:Cross(el)
		local eye = centre + n * r
		local w, h = 0.12 * r, 0.19 * r * open
		if i < 2 then
			if open > 0.5 and eyes:GetBool() then
				local s = 0.035 * r
				piece(eye + n * 0.05 * r + ey * h * 0.4 - el * w * 0.35, el, ey, n, s, s, s, 0.05, 0.05, 0.05)
			end
		elseif i < 4 then
			if eyes:GetBool() then piece(eye, el, ey, n, w, h, 0.06 * r, 1, 1, 1) end
		elseif brows:GetBool() and upset > 0.2 then
			local roll = -side * 0.45 * upset
			local at = centre + (fwd + up * (0.42 + 0.06 * upset) + left * side * 0.3):GetNormalized() * r
			local c, s = math.cos(roll), math.sin(roll)
			local bx, by = el * c + ey * s, ey * c - el * s
			piece(at, bx, by, n, 0.24 * r, 0.035 * r, 0.03 * r, 0.2, 0.12, 0.05)
		end
	end
	render.MaterialOverride()
	render.SetColorModulation(1, 1, 1)
end

hook.Add("PostDrawOpaqueRenderables", "DMS_Face", function(depth, sky)
	if sky or not (eyes:GetBool() or brows:GetBool()) then return end
	for _, ent in ipairs(ents.FindByClass("prop_ragdoll")) do
		if ent:GetNW2Bool("DMS_Homer") and not ent:IsDormant() then draw(ent) end
	end
end)
