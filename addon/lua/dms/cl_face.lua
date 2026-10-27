-- Homer's face as the Playroom draws it (`delirium-room` `face`, ROADMAP M6): dark eyes with a white glint and dark
-- brows on his head capsule. The eyes blink, look at where he was last hit (or along his way when he's quick), squint with pain, shut
-- when he's dead or blinking; the brows show when he's scared, worried or hurt. The server sends NM's view (NW2 vars,
-- reactions.lua); the head is the bone's, so the face moves with the drawn ragdoll.

local eyes = CreateClientConVar("dms_eyes", "1", true, false, "Homer's eyes (the Playroom's)")
local brows = CreateClientConVar("dms_brows", "1", true, false, "Homer's eyebrows (the Playroom's)")

-- One of the Playroom's pieces, a unit sphere: at `pos`, its axes x y z scaled by sx sy sz (units), in a colour.
-- Built here in the world's frame (a mesh of rings), as no matrix is trusted to reach render.DrawSphere.
local RINGS, SEGS = 8, 12
local function piece(pos, x, y, z, sx, sy, sz, r, g, b)
	if sx <= 0 or sy <= 0 then return end
	local function at(i, j)
		local th, ph = math.pi * i / RINGS, 2 * math.pi * j / SEGS
		local s = math.sin(th)
		return pos + x * (sx * s * math.cos(ph)) + y * (sy * s * math.sin(ph)) + z * (sz * math.cos(th))
	end
	local c = Color(r * 255, g * 255, b * 255)
	-- Both windings: seen from either side whatever the material culls.
	mesh.Begin(MATERIAL_TRIANGLES, RINGS * SEGS * 4)
	for i = 0, RINGS - 1 do
		for j = 0, SEGS - 1 do
			local a, b2, c2, d = at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)
			for _, p in ipairs({ a, b2, c2, a, c2, d, a, c2, b2, a, d, c2 }) do
				mesh.Position(p)
				mesh.Color(c.r, c.g, c.b, 255)
				mesh.AdvanceVertex()
			end
		end
	end
	mesh.End()
end

-- A bar (the Playroom's unit cube scaled): its half sizes along x y z, both windings.
local FACES = { { 1, 2, 4, 3 }, { 5, 7, 8, 6 }, { 1, 5, 6, 2 }, { 3, 4, 8, 7 }, { 1, 3, 7, 5 }, { 2, 6, 8, 4 } }
local function bar(pos, x, y, z, hx, hy, hz, r, g, b)
	local v = {}
	for i = 0, 7 do
		v[i + 1] = pos + x * (i % 2 == 0 and -hx or hx) + y * (math.floor(i / 2) % 2 == 0 and -hy or hy) + z * (i < 4 and -hz or hz)
	end
	mesh.Begin(MATERIAL_TRIANGLES, #FACES * 4)
	for _, f in ipairs(FACES) do
		local a, b2, c2, d = v[f[1]], v[f[2]], v[f[3]], v[f[4]]
		for _, p in ipairs({ a, b2, c2, a, c2, d, a, c2, b2, a, d, c2 }) do
			mesh.Position(p)
			mesh.Color(r * 255, g * 255, b * 255, 255)
			mesh.AdvanceVertex()
		end
	end
	mesh.End()
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
	render.SetColorMaterial()
	-- The eyes, their glints over them, the brows (the colour material writes no depth: order is what shows).
	for _, i in ipairs({ 2, 3, 0, 1, 4, 5 }) do
		local side = i % 2 == 0 and 1 or -1
		local n = (fwd + up * 0.1 + left * side * 0.3 + gaze):GetNormalized()
		local el = up:Cross(n):GetNormalized()
		local ey = n:Cross(el)
		local eye = centre + n * r
		local w, h = 0.12 * r, 0.19 * r * open
		-- The Playroom's: the eye a dark ellipsoid, its glint a small white ball, the brow a dark bar.
		if i < 2 then
			if open > 0.5 and eyes:GetBool() then
				local s = 0.035 * r
				piece(eye + n * 0.05 * r + ey * h * 0.4 - el * w * 0.35, el, ey, n, s, s, s, 1, 1, 1)
			end
		elseif i < 4 then
			if eyes:GetBool() then piece(eye, el, ey, n, w, h, 0.06 * r, 0.05, 0.05, 0.07) end
		elseif brows:GetBool() and upset > 0.2 then
			local roll = -side * 0.45 * upset
			local d = (fwd + up * (0.42 + 0.06 * upset) + left * side * 0.3):GetNormalized()
			-- The Playroom draws its face over the body; here it's on the head capsule's surface that way (its
			-- straight part reaches past r off the middle), so the brow isn't inside the head.
			local hl, a = ent:GetNW2Float("DMS_FaceHalf", 0), d:Dot(up)
			local side2 = math.max(1 - a * a, 1e-4)
			local t = r / math.sqrt(side2)
			if math.abs(t * a) > hl then t = hl * math.abs(a) + math.sqrt(math.max(r * r - hl * hl * side2, 0)) end
			local at = centre + d * math.max(t, r)
			local c, s = math.cos(roll), math.sin(roll)
			local bx, by = el * c + ey * s, ey * c - el * s
			bar(at, bx, by, n, 0.12 * r, 0.0175 * r, 0.015 * r, 0.05, 0.05, 0.07)
		end
	end
end

local failed
-- After the see-through body, as the Playroom has the face over it.
hook.Add("PostDrawTranslucentRenderables", "DMS_Face", function(depth, sky)
	if sky or not (eyes:GetBool() or brows:GetBool()) then return end
	for _, ent in ipairs(ents.FindByClass("prop_ragdoll")) do
		if ent:GetNW2Bool("DMS_Homer") and not ent:IsDormant() then
			local ok, err = pcall(draw, ent)
			if not ok and err ~= failed then
				failed = err
				print("[DMS] face: " .. tostring(err))
			end
		end
	end
end)

-- dms_face_debug: what the face has to go on, Homer by Homer.
concommand.Add("dms_face_debug", function()
	local n = 0
	for _, ent in ipairs(ents.FindByClass("prop_ragdoll")) do
		local bone = ent:GetNW2Int("DMS_HeadBone", -1)
		local bp, ba = ent:GetBonePosition(math.max(bone, 0))
		print(string.format("[DMS] %s homer=%s dormant=%s head bone %d (%s) at %s %s, r %.2f, centre %s, fwd %s, up %s, last error %s",
			tostring(ent), tostring(ent:GetNW2Bool("DMS_Homer")), tostring(ent:IsDormant()), bone, tostring(bone >= 0 and ent:GetBoneName(bone)),
			tostring(bp), tostring(ba), ent:GetNW2Float("DMS_FaceR", 0), tostring(ent:GetNW2Vector("DMS_FaceCentre")),
			tostring(ent:GetNW2Vector("DMS_FaceFwd")), tostring(ent:GetNW2Vector("DMS_FaceUp")), tostring(failed)))
		print(string.format("[DMS]   last hit: bone %d, %.2f s ago, at %s; speed %.2f m/s; dead %s", ent:GetNW2Int("DMS_RingBone", -1),
			CurTime() - ent:GetNW2Float("DMS_RingTime", -100), tostring(ent:GetNW2Vector("DMS_RingOn")), ent:GetNW2Vector("DMS_V"):Length(),
			tostring(ent:GetNW2Bool("DMS_Dead"))))
		n = n + 1
	end
	print("[DMS] " .. n .. " ragdoll(s)")
end)
