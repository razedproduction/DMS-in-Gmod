-- Drawn over Homer (ROADMAP M6): the Playroom's skeleton (`skeleton`: lines joint to joint, a sphere a joint coloured
-- by side, the leaves' tips, a yellow ball on the parts NM sees touching) and, for superadmins, the debug overlay
-- from M4: NM's capsules and boxes over the ragdoll, the drives' forces and torques, the centre of mass and the
-- balancer's state. Shapes come once (dms_shape), NM's view 15 times a second (dms_overlay, overlay.lua).

local skeleton = CreateClientConVar("dms_skeleton", "1", true, false, "The Playroom's skeleton over Homer")
local overlay = CreateClientConVar("dms_overlay", "0", false, false, "NM's capsules, drives and centre of mass over Homer (superadmins)")

local N = 25
local shape
local views = {} -- ent -> {at, data}

net.Receive("dms_shape", function()
	local n = net.ReadUInt(32)
	shape = util.JSONToTable(util.Decompress(net.ReadData(n)) or "")
end)

net.Receive("dms_overlay", function()
	local ent = net.ReadEntity()
	local n = net.ReadUInt(16)
	local d = {}
	for i = 1, n do d[i] = net.ReadFloat() end
	if IsValid(ent) then views[ent] = { at = CurTime(), d = d } end
end)

local function subscribe(on)
	net.Start("dms_overlay")
	net.WriteBool(on)
	net.SendToServer()
end
cvars.AddChangeCallback("dms_overlay", function(_, _, v) subscribe(v ~= "0") end, "DMS_Overlay")

local nextAsk = 0
local function needShape()
	if shape or CurTime() < nextAsk then return end
	nextAsk = CurTime() + 2
	net.Start("dms_shape")
	net.SendToServer()
end

-- The Playroom's `side_color`.
local RED, BLUE, GREY = Color(217, 77, 64), Color(77, 115, 230), Color(184, 184, 189)
local function sideColour(name)
	if name:find("Left$") then return RED end
	if name:find("Right$") then return BLUE end
	return GREY
end
local YELLOW = Color(255, 217, 26)

local function bone(ent, s)
	local p, a = ent:GetBonePosition(s.bone)
	return p, a
end

local function drawSkeleton(ent, view)
	local joints, frames = {}, {}
	for k, s in ipairs(shape) do
		local p, a = bone(ent, s)
		if not p then return end
		joints[k], frames[k] = p, { p, a }
	end
	render.SetColorMaterial()
	for k, s in ipairs(shape) do
		local x = joints[k]
		local c = sideColour(s.name)
		if s.parent >= 0 then render.DrawLine(x, joints[s.parent + 1], color_white, false) end
		local leaf = true
		for _, o in ipairs(shape) do if o.parent == k - 1 then leaf = false break end end
		local centre = LocalToWorld(s.centre, angle_zero, frames[k][1], frames[k][2])
		if s.parent >= 0 and leaf then
			local tip = centre * 2 - x
			render.DrawLine(x, tip, color_white, false)
			render.DrawSphere(tip, 0.015 / 0.0254, 6, 6, c)
		end
		render.DrawSphere(x, 0.022 / 0.0254, 6, 6, c)
		if view and view.d[(k - 1) * N + 25] == 1 then
			render.DrawSphere(centre, 0.045 / 0.0254, 8, 8, YELLOW)
		end
	end
end

local function drawDebug(ent, view)
	local d = view.d
	render.SetColorMaterial()
	for k, s in ipairs(shape) do
		local i = (k - 1) * N
		local p, a = bone(ent, s)
		if not p then return end
		local w = function(v) return LocalToWorld(v, angle_zero, p, a) end
		local c = ColorAlpha(sideColour(s.name), 90)
		if s.box.x > 0 then
			local x = LocalToWorld(s.x, angle_zero, vector_origin, a)
			local ang = x:AngleEx(LocalToWorld(s.z, angle_zero, vector_origin, a))
			local centre = (w(s.a) + w(s.b)) * 0.5
			render.DrawWireframeBox(centre, ang, -s.box, s.box, c, true)
		else
			local A, B = w(s.a), w(s.b)
			render.DrawSphere(A, s.r, 10, 10, c)
			render.DrawSphere(B, s.r, 10, 10, c)
			render.DrawLine(A, B, c, false)
		end
		-- The drive: force (green, 1 cm a N) from the part's centre, torque (purple, 1 cm a N m).
		local centre = LocalToWorld(s.centre, angle_zero, p, a)
		local f = Vector(d[i + 19], d[i + 20], d[i + 21])
		local t = Vector(d[i + 22], d[i + 23], d[i + 24])
		render.DrawLine(centre, centre + f * (0.01 / 0.0254), Color(60, 220, 90), false)
		render.DrawLine(centre, centre + t * (0.01 / 0.0254), Color(190, 90, 230), false)
	end
	local base = #shape * N
	local com = Vector(d[base + 1], d[base + 2], d[base + 3])
	local state = d[base + 4]
	local col = state == 2 and Color(230, 60, 60) or state == 1 and Color(60, 220, 90) or Color(230, 200, 60)
	render.DrawSphere(com, 2.5, 10, 10, col)
	-- Its projection on the floor under it: the stance's centre of mass over the feet.
	local tr = util.TraceLine({ start = com, endpos = com - Vector(0, 0, 200), filter = ent, mask = MASK_SOLID_BRUSHONLY })
	render.DrawLine(com, tr.HitPos, col, false)
	render.DrawWireframeSphere(tr.HitPos, 4, 8, 2, col, false)
end

hook.Add("PostDrawTranslucentRenderables", "DMS_Overlay", function(depth, sky)
	if sky then return end
	local skel, dbg = skeleton:GetBool(), overlay:GetBool()
	if not (skel or dbg) then return end
	for _, ent in ipairs(ents.FindByClass("prop_ragdoll")) do
		if ent:GetNW2Bool("DMS_Homer") and not ent:IsDormant() then
			needShape()
			if shape then
				local view = views[ent]
				if view and CurTime() - view.at > 1 then view = nil end
				if skel then drawSkeleton(ent, view) end
				if dbg and view then drawDebug(ent, view) end
			end
		end
	end
end)

hook.Add("InitPostEntity", "DMS_Overlay", function()
	if overlay:GetBool() then subscribe(true) end
end)
