-- ROADMAP M9 (docs/adr/0025): the spikes' prototypes for GMod's ragdolls, superadmins' commands only. Homer's own
-- path (homer.lua, follow.lua) is untouched. A model's ValveBiped skeleton is read from its .mdl (DMS.ModelSkeleton),
-- Homer's build fitted to it (DMS.FitModel), and an NM body made for it in one of S3's two ways:
--   (b) proxy:  a ragdoll of the fitted body compiled by `dms-homer --joints` into models/dms/proxy/ (as homer.mdl);
--   (a) native: the fork's own bodies and joints for the fitted body (IPhysicsNM VBOX3D_NM_005).
-- Either way NM drives that body in FOLLOW as it drives Homer, and the model's own prop_ragdoll is posed from its
-- parts (S4): its physics objects frozen, without collisions, put where their bones are on the parts every tick.

local METRES_PER_UNIT = 0.0254
local say = DMS_Say

-- Body entity -> { model =, look = the model's ragdoll, parts = its 21 PhysObj or nil (native) }
DMS_SpikeBodies = DMS_SpikeBodies or {}

local function loadData()
	if DMS.DataInfo() then return true end
	return pcall(DMS.LoadData, DMS_GameDir())
end

-- Homer's build fitted to the model, kept by the module under the model's path. The fit's table, or nil and why.
function DMS_SpikeFit(model)
	if not loadData() then return nil, "NM's data isn't loaded" end
	local bytes = file.Read(model, "GAME")
	if not bytes then return nil, model .. ": not found" end
	local t0 = SysTime()
	local ok, f = pcall(DMS.FitModel, model, bytes)
	if not ok then return nil, tostring(f) end
	f.ms = (SysTime() - t0) * 1000
	return f
end

-- Where the proxy of a model goes (b): models/dms/proxy/<the model's path with _>.mdl.
function DMS_SpikeProxy(model)
	return "models/dms/proxy/" .. model:gsub("^models/", ""):gsub("%.mdl$", ""):gsub("[/\\]", "_") .. ".mdl"
end

-- The model's own ragdoll, drawn and traced, its objects frozen and without collisions: posed from the NM body.
local function spawnLook(model, pos, yaw)
	local look = ents.Create("prop_ragdoll")
	look:SetModel(model)
	look:SetPos(pos)
	look:SetAngles(Angle(0, yaw, 0))
	look:Spawn()
	look:Activate()
	for i = 0, look:GetPhysicsObjectCount() - 1 do
		local p = look:GetPhysicsObjectNum(i)
		p:EnableMotion(false)
		p:EnableCollisions(false)
		p:EnableGravity(false)
	end
	return look
end

local function track(ent, model, look, parts)
	DMS_SpikeBodies[ent] = { model = model, look = look, parts = parts }
	look.DMS_SpikeBody = ent
	ent:CallOnRemove("DMS_SpikeLook", function(e)
		local s = DMS_SpikeBodies[e]
		DMS_SpikeBodies[e] = nil
		if s and IsValid(s.look) then s.look:Remove() end
	end)
	look:CallOnRemove("DMS_SpikeBody", function(l) if IsValid(l.DMS_SpikeBody) then l.DMS_SpikeBody:Remove() end end)
end

-- (b): the proxy ragdoll at pos facing yaw (degrees), bound to the fitted body, NM on it, and the model posed from it.
-- The proxy entity and the model's, or nil and why.
function DMS_SpikeSpawnProxy(model, pos, yaw, showProxy)
	local proxy = DMS_SpikeProxy(model)
	if not util.IsValidModel(proxy) then return nil, proxy .. " isn't there: dms_spike_joints, then dms-homer --joints" end
	local f, why = DMS_SpikeFit(model)
	if not f then return nil, why end
	DMS_MapWorld()
	local ent = ents.Create("prop_ragdoll")
	ent:SetModel(proxy)
	ent:SetPos(pos)
	ent:SetAngles(Angle(0, yaw, 0))
	ent:Spawn()
	ent:Activate()
	local parts, err = DMS_HomerParts(ent)
	if not parts then ent:Remove() return nil, err end
	local ok, e = pcall(DMS.BindFitted, model, parts)
	if not ok then ent:Remove() return nil, "bind: " .. tostring(e) end
	local ok2, id = pcall(DMS.FollowFitted, model, parts, math.rad(yaw), pos * METRES_PER_UNIT, false)
	if not ok2 then ent:Remove() return nil, "follow: " .. tostring(id) end
	ent.DMS_Agent, ent.DMS_Spawn, ent.DMS_Bound = id, { pos = pos, yaw = yaw }, true
	DMS_Agents[id] = ent
	ent:CallOnRemove("DMS_Unfollow", function(e2) DMS_Unfollow(e2) end)
	if not showProxy then ent:SetNoDraw(true) end
	local look = spawnLook(model, pos, yaw)
	track(ent, model, look, parts)
	return ent, look
end

-- S4 on the server: the model's physics objects where their bones are on the NM body's parts, every tick.
hook.Add("Tick", "DMS_SpikePose", function()
	for ent, s in pairs(DMS_SpikeBodies) do
		if not IsValid(ent) or not IsValid(s.look) then
			DMS_SpikeBodies[ent] = nil
		else
			local ok, pose = pcall(DMS.PoseModel, s.model, s.parts or ent.DMS_Native)
			if ok then
				local look = s.look
				for i = 0, look:GetPhysicsObjectCount() - 1 do
					local b = pose[look:GetBoneName(look:TranslatePhysBoneToBone(i))]
					if b then
						local p = look:GetPhysicsObjectNum(i)
						p:SetPos(Vector(b.pos.x, b.pos.y, b.pos.z), true)
						p:SetAngles(Angle(b.ang.x, b.ang.y, b.ang.z))
					end
				end
			end
		end
	end
end)

local function admin(ply) return not IsValid(ply) or ply:IsSuperAdmin() end

local function inFront(ply)
	if not IsValid(ply) then return Vector(0, 0, 0), 0 end
	local tr = util.TraceLine({ start = ply:EyePos(), endpos = ply:EyePos() + ply:GetAimVector() * 150, filter = ply })
	return DMS_FloorAt(tr.HitPos, ply), ply:EyeAngles().y + 180
end

-- dms_spike_fit <model>: the model's skeleton fitted, its sizes and masses.
concommand.Add("dms_spike_fit", function(ply, _, args)
	if not admin(ply) then return end
	local model = args[1] or "models/humans/group01/male_07.mdl"
	local f, why = DMS_SpikeFit(model)
	if not f then return say(ply, "[DMS] " .. tostring(why)) end
	say(ply, string.format("[DMS] %s: %.3f m, %.1f kg (Homer %.3f m, %.1f kg), %.2f ms", model, f.height, f.mass, f.homer_height,
		f.homer_mass, f.ms))
end)

-- dms_spike_joints <model>: the fitted joints into data/dms/spike/<proxy name>.txt, for dms-homer --joints.
concommand.Add("dms_spike_joints", function(ply, _, args)
	if not admin(ply) then return end
	local model = args[1] or "models/humans/group01/male_07.mdl"
	local f, why = DMS_SpikeFit(model)
	if not f then return say(ply, "[DMS] " .. tostring(why)) end
	file.CreateDir("dms/spike")
	local name = "dms/spike/" .. string.GetFileFromFilename(DMS_SpikeProxy(model)):gsub("%.mdl$", ".txt")
	file.Write(name, DMS.FitJoints(model))
	say(ply, "[DMS] data/" .. name .. " -> dms-homer --joints <it> --model " .. DMS_SpikeProxy(model):gsub("^models/", ""))
end)

-- dms_spike_proxy <model> [show]: (b) in front of the player, under bodyBalance.
concommand.Add("dms_spike_proxy", function(ply, _, args)
	if not admin(ply) then return end
	local model = args[1] or "models/humans/group01/male_07.mdl"
	local pos, yaw = inFront(ply)
	local ent, look = DMS_SpikeSpawnProxy(model, pos, yaw, args[2] == "show")
	if not ent then return say(ply, "[DMS] " .. tostring(look)) end
	DMS_Send(ent, "bodyBalance start=true")
	say(ply, string.format("[DMS] %s on its proxy, NM agent #%d, bodyBalance", model, ent.DMS_Agent))
end)
