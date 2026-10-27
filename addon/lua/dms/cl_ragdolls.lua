-- GMod's ragdolls with NM on them (ROADMAP M10, ragdolls.lua), the clients' part: the server poses the model's physics
-- objects from NM's body, and the bones NM has a part for but the model has no object for (Spine, Spine1, Spine4,
-- Neck1, the clavicles) are put right here as it's drawn: each in its physics parent's frame as the server
-- sends it (DMS_FP<k>, DMS_FA<k>), the bones under it that have no object of their own carried along.

-- The bones finished, each kept to a physics bone's frame (ragdolls.lua has the same list).
local FINISHED = {
	{ "ValveBiped.Bip01_Spine", "ValveBiped.Bip01_Pelvis" },
	{ "ValveBiped.Bip01_Spine1", "ValveBiped.Bip01_Pelvis" },
	{ "ValveBiped.Bip01_Spine4", "ValveBiped.Bip01_Spine2" },
	{ "ValveBiped.Bip01_Neck1", "ValveBiped.Bip01_Spine2" },
	{ "ValveBiped.Bip01_L_Clavicle", "ValveBiped.Bip01_Spine2" },
	{ "ValveBiped.Bip01_R_Clavicle", "ValveBiped.Bip01_Spine2" },
}

-- The model's bones: which are finished (and from which parent), which have an object.
local function layout(ent)
	local fin, phys = {}, {}
	for k, f in ipairs(FINISHED) do
		local b, p = ent:LookupBone(f[1]), ent:LookupBone(f[2])
		if b and p then fin[b] = { k = k, parent = p } end
	end
	for i = 0, ent:GetBoneCount() - 1 do
		local pb = ent:TranslateBoneToPhysBone(i)
		phys[i] = pb and pb >= 0 and ent:TranslatePhysBoneToBone(pb) == i or nil
	end
	return { fin = fin, phys = phys }
end

local function finish(ent, count)
	if not ent:GetNW2Bool("DMS_Ragdoll") then return end
	local l = ent.DMS_Layout
	if not l then
		l = layout(ent)
		ent.DMS_Layout = l
	end
	local delta = {}
	for i = 0, count - 1 do
		local f = l.fin[i]
		if f then
			local pm, old = ent:GetBoneMatrix(f.parent), ent:GetBoneMatrix(i)
			if pm and old then
				local m = Matrix()
				m:SetTranslation(ent:GetNW2Vector("DMS_FP" .. f.k))
				m:SetAngles(ent:GetNW2Angle("DMS_FA" .. f.k))
				local new = pm * m
				ent:SetBoneMatrix(i, new)
				delta[i] = new * old:GetInverseTR()
			end
		elseif not l.phys[i] then
			local p = ent:GetBoneParent(i)
			local d = p and p >= 0 and delta[p]
			local old = d and ent:GetBoneMatrix(i)
			if old then
				ent:SetBoneMatrix(i, d * old)
				delta[i] = d
			end
		end
	end
end

-- A server ragdoll's bones on the client (C_ServerRagdoll) are built without the BuildBonePositions callback, so
-- they're finished as it's drawn: set up, put right, then drawn from the bones as they are now (a model already
-- overridden is drawn by its own override after).
local function render(ent, flags)
	ent:SetupBones()
	finish(ent, ent:GetBoneCount())
	if ent.DMS_OldRender then return ent.DMS_OldRender(ent, flags) end
	ent:DrawModel(flags)
end

-- A player's death ragdoll (deaths.lua) in the player's colour: the PlayerColor material proxy asks the entity.
local function playerColor(ent) return ent:GetNW2Vector("DMS_PlayerColor") end

local nextLook = 0
hook.Add("Think", "DMS_Ragdolls", function()
	if CurTime() < nextLook then return end
	nextLook = CurTime() + 0.25
	for _, ent in ipairs(ents.FindByClass("prop_ragdoll")) do
		if not ent.GetPlayerColor and IsValid(ent:GetNW2Entity("DMS_Owner")) then ent.GetPlayerColor = playerColor end
		local on = ent:GetNW2Bool("DMS_Ragdoll")
		if on and not ent.DMS_Finishing then
			ent.DMS_Finishing, ent.DMS_OldRender = true, ent.RenderOverride
			ent.RenderOverride = render
		elseif not on and ent.DMS_Finishing then
			ent.DMS_Finishing, ent.RenderOverride, ent.DMS_OldRender = nil, ent.DMS_OldRender, nil
		end
	end
end)

-- The death camera on a player's server death ragdoll, as GMod's (HL2MP's) follows its client one: over the
-- ragdoll's root, CHASE units back along the view, stopped by walls; the mouse turns it.
local CHASE, DEAD_VIEW, WALL = 96, 14, Vector(6, 6, 6)
local PELVIS = "ValveBiped.Bip01_Pelvis"
hook.Add("CalcView", "DMS_DeathCamera", function(ply, _, angles, fov)
	if ply:Alive() then return end
	local mode = ply:GetObserverMode()
	if mode ~= OBS_MODE_NONE and mode ~= OBS_MODE_DEATHCAM and mode ~= OBS_MODE_FREEZECAM then return end
	local rag = ply:GetNW2Entity("DMS_Corpse")
	if not IsValid(rag) then return end
	local b = rag:LookupBone(PELVIS)
	local root = b and rag:GetBonePosition(b) or rag:GetPos()
	local eye = root + Vector(0, 0, DEAD_VIEW)
	local tr = util.TraceHull({ start = eye, endpos = eye - angles:Forward() * CHASE, mins = -WALL, maxs = WALL, mask = MASK_SOLID, filter = { ply, rag } })
	return { origin = tr.HitPos, angles = angles, fov = fov, drawviewer = false }
end)
