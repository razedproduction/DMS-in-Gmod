-- Death ragdolls (ROADMAP M11, docs/adr/0028): an NPC or a player with a ValveBiped model that dies leaves a server
-- ragdoll with NM on it at once (ragdolls.lua), in the pose and moving as it died, and the killing hit goes to it as the
-- Playroom's room takes one (follow/room.rs) on a fatally injured body (health 49): its ShotTask's dying reaction
-- (`shot`, `shotRelax`), no getting up, later hits reacting till its health is 0 (the room's death: `STOP all`, `relax`).
--
-- NPCs: their ragdoll is made on the server for it (SetShouldServerRagdoll, as ai_serverragdolls 1 makes them all),
-- without the game's push of the killing hit (the room's impulse is it). Players: GMod's death ragdoll is the client's
-- (no FOLLOW there, docs/adr/0002), so it's taken away and a server ragdoll of the player's model is made in its place,
-- posed from the player's bones; the client's camera follows it as GMod's death camera follows its own.
--
-- At most dms_autoattach_max of them have NM (and dms_max_agents in all): a new one takes NM off the oldest, as does
-- dms_autoattach_time; NM's off means an ordinary ragdoll in its place (DMS_DetachRagdoll). Removed (a fade, cleanup,
-- undo, the player's next death), NM goes with it (ragdolls.lua's OnRemove).

local M = 0.0254

local npcs = CreateConVar("dms_autoattach_npc", "1", { FCVAR_ARCHIVE, FCVAR_REPLICATED },
	"NM on the death ragdolls of NPCs with ValveBiped models (made on the server for it), reacting to the killing hit")
local players = CreateConVar("dms_autoattach_players", "1", { FCVAR_ARCHIVE, FCVAR_REPLICATED },
	"NM on players' death ragdolls: a server ragdoll of the player's model in place of GMod's client one, the death camera on it")
local maxDeaths = CreateConVar("dms_autoattach_max", "4", FCVAR_ARCHIVE,
	"Death ragdolls with NM at once (within dms_max_agents): a new one takes NM off the oldest")
local keepFor = CreateConVar("dms_autoattach_time", "30", FCVAR_ARCHIVE,
	"Seconds a death ragdoll keeps NM before it's an ordinary ragdoll (0: till dms_autoattach_max takes it)")

-- How recent the last hit has to be to be the killing one (s).
local HIT_AGE = 0.5
-- The game's push of a killing hit on an NPC, kept out of its ragdoll: Source makes up its own for a zero force
-- (CalcDamageForceVector), so a negligible one (N s) stands in.
local NO_PUSH = Vector(0, 0, 1e-4)

local function at(p) return string.format("%.4f,%.4f,%.4f", p.x * M, p.y * M, p.z * M) end
local function dir(d) return string.format("%.4f,%.4f,%.4f", d.x, d.y, d.z) end

-- GMod's ragdolls work with this fork (VBOX3D_NM_006) and a limit above 0.
local function possible()
	if maxDeaths:GetInt() <= 0 then return false end
	local iface = DMS.InterfaceVersion()
	return iface ~= nil and (tonumber(iface:match("(%d+)$")) or 0) >= 6
end

local function wanted(ent)
	if ent:IsPlayer() then return players:GetBool() end
	return ent:IsNPC() and npcs:GetBool()
end

-- The hit as the room's reactions take it (reactions.lua's translation): kind, where, which way, the weapon.
local function hitOf(ent, dmg)
	local pos = dmg:GetDamagePosition()
	if pos:IsZero() then pos = ent:WorldSpaceCenter() end
	local att, force = dmg:GetAttacker(), dmg:GetDamageForce()
	local from = IsValid(att) and (att.EyePos and att:EyePos() or att:GetPos()) or pos
	local h = { pos = pos, time = CurTime(), damage = dmg:GetDamage(), force = force, from = from }
	if dmg:IsBulletDamage() or dmg:IsDamageType(DMG_BUCKSHOT) then
		local d = force:IsZero() and (pos - from) or force
		h.kind, h.dir, h.weapon = "shot", d:GetNormalized(), DMS_WeaponOf(dmg)
	elseif dmg:IsExplosionDamage() then
		h.kind, h.centre = "blast", DMS_BlastCentre(ent, dmg, pos)
	elseif dmg:IsDamageType(DMG_BURN) or dmg:IsDamageType(DMG_SLOWBURN) then
		h.kind = "burn"
	elseif dmg:IsDamageType(DMG_CLUB) or dmg:IsDamageType(DMG_SLASH) then
		h.kind = "melee"
	elseif dmg:IsDamageType(DMG_SHOCK) then
		h.kind = "taser"
	end
	return h
end
DMS_HitOf = hitOf

-- The room's lines for the killing hit `h` on the ragdoll: the room's fatally injured (health under 50, its ShotTask's
-- dying reaction, no getting up), then the hit as the room takes it, its damage on that health (at 0 the room's death).
-- No hit known (a fall, a crush, `kill`): just its death.
local FATALLY_INJURED = 49
local function deathLines(rag, h)
	if not h or CurTime() - h.time > HIT_AGE then return { "health value=0" } end
	local hit
	if h.kind == "shot" then
		local part = DMS_RagdollPartAt(rag, h.pos, h.dir)
		hit = string.format("shot at=%s dir=%s weapon=%s%s", at(h.pos), dir(h.dir), h.weapon, part and (" part=" .. part) or "")
	elseif h.kind == "blast" then
		hit = "blast at=" .. at(h.centre)
	elseif h.kind == "burn" then
		hit = "burn"
	elseif h.kind == "melee" then
		hit = string.format("melee damage=%.2f at=%s impulse=%s head=%s", h.damage, at(h.pos), at(h.force), at(h.from))
	elseif h.kind == "taser" then
		hit = "taser at=" .. at(h.pos)
	end
	if not hit then return { "health value=0" } end
	return { "health value=" .. FATALLY_INJURED, hit }
end

-- The death ragdolls NM is on, oldest first: {ent, owner (a player), since}.
local corpses = {}

local function prune()
	for i = #corpses, 1, -1 do
		local c = corpses[i]
		if not IsValid(c.ent) or not c.ent.DMS_Agent then table.remove(corpses, i) end
	end
end

-- A player's camera on their death ragdoll (cl_ragdolls.lua).
local function follow(ply, rag)
	if not IsValid(ply) then return end
	ply.DMS_Corpse = rag
	ply:SetNW2Entity("DMS_Corpse", rag)
end

-- NM off a death ragdoll: an ordinary ragdoll in its place, a player's camera moved to it.
local function release(c)
	if not IsValid(c.ent) or not c.ent.DMS_Agent then return end
	local old = c.ent
	local color, owner = old:GetNW2Vector("DMS_PlayerColor"), old:GetNW2Entity("DMS_Owner")
	local new = DMS_DetachRagdoll(old)
	if not IsValid(new) then return end
	if IsValid(owner) then
		new:SetNW2Vector("DMS_PlayerColor", color)
		new:SetNW2Entity("DMS_Owner", owner)
	end
	if IsValid(c.owner) and c.owner.DMS_Corpse == old then follow(c.owner, new) end
end

-- Room for one more: the oldest death ragdolls let go of while there are dms_autoattach_max of them or dms_max_agents
-- bodies have NM (not asked for a ragdoll that has NM already, `had`). Whether there's room.
local function makeRoom(had)
	prune()
	while #corpses > 0 and (#corpses >= maxDeaths:GetInt() or not (had or DMS_AgentRoom())) do
		release(table.remove(corpses, 1))
	end
	return #corpses < maxDeaths:GetInt() and (had or DMS_AgentRoom())
end

-- NM on a death ragdoll (kept on one that has it, live.lua's) and the room's lines to it (by default the killing hit
-- `h`'s). The agent's id, or nil and why.
local function attach(rag, h, owner, lines)
	if not makeRoom(rag.DMS_Agent ~= nil) then return nil, "no room (dms_autoattach_max, dms_max_agents)" end
	local id, why = DMS_AttachRagdoll(rag)
	if not id then return nil, why end
	rag.DMS_Death = true
	corpses[#corpses + 1] = { ent = rag, owner = owner, since = CurTime() }
	for _, line in ipairs(lines or deathLines(rag, h)) do DMS_React(rag, line) end
	hook.Run("DMS_DeathRagdoll", rag, owner)
	return id
end

-- The killing hit, and for an NPC its ragdoll made on the server without the game's push.
hook.Add("EntityTakeDamage", "DMS_Deaths", function(ent, dmg)
	if not (ent:IsNPC() or ent:IsPlayer()) or not wanted(ent) or not DMS_HasValveBiped(ent) then return end
	ent.DMS_LastHit = hitOf(ent, dmg)
	if ent:IsNPC() and possible() and dmg:GetDamage() >= ent:Health() then
		ent:SetShouldServerRagdoll(true)
		local h = ent.DMS_LastHit.kind
		if h == "shot" or h == "blast" or h == "melee" then dmg:SetDamageForce(NO_PUSH) end
	end
end)

-- An NPC's server ragdoll: NM on it, posed and moving as the NPC died (the game's own ragdolling, ADR 0027's swap).
hook.Add("CreateEntityRagdoll", "DMS_Deaths", function(owner, rag)
	-- One that died as a ragdoll (live.lua): that ragdoll is its corpse.
	if not IsValid(owner) or owner.DMS_Live or not owner:IsNPC() or not npcs:GetBool() or not possible() then return end
	if not IsValid(rag) or rag:GetClass() ~= "prop_ragdoll" or not DMS_IsValveBiped(rag) then return end
	local h, name = owner.DMS_LastHit, tostring(owner)
	-- Undo and cleanup take the corpse in the NPC's place.
	undo.ReplaceEntity(owner, rag)
	cleanup.ReplaceEntity(owner, rag)
	-- The game gives the ragdoll its objects' motion after this hook: NM takes it over a tick later.
	timer.Simple(0, function()
		if not IsValid(rag) or rag.DMS_Agent then return end
		local id, why = attach(rag, h)
		if not id and why then print("[DMS] NM isn't on " .. name .. "'s death ragdoll: " .. tostring(why)) end
	end)
end)

-- A ragdoll of a player's or an NPC's model as it looks, posed from its bones and moving as it was (a player's death
-- ragdoll, live.lua's). The ragdoll, or nil for a model without a ValveBiped one.
function DMS_RagdollFrom(ent)
	local rag = ents.Create("prop_ragdoll")
	rag:SetModel(ent:GetModel())
	rag:SetPos(ent:GetPos())
	rag:SetAngles(Angle(0, ent:GetAngles().y, 0))
	rag:SetSkin(ent:GetSkin())
	for i = 0, ent:GetNumBodyGroups() - 1 do rag:SetBodygroup(i, ent:GetBodygroup(i)) end
	rag:SetColor(ent:GetColor())
	rag:SetRenderMode(ent:GetRenderMode())
	rag:SetMaterial(ent:GetMaterial())
	for i = 0, #(ent:GetMaterials() or {}) - 1 do
		local m = ent:GetSubMaterial(i)
		if m ~= "" then rag:SetSubMaterial(i, m) end
	end
	rag:Spawn()
	rag:Activate()
	if not DMS_IsValveBiped(rag) then
		rag:Remove()
		return nil
	end
	-- As GMod's own: the world and things, not players and NPCs.
	rag:SetCollisionGroup(COLLISION_GROUP_WEAPON)
	if ent:IsPlayer() then
		-- The player's colour on their model (the PlayerColor material proxy, cl_ragdolls.lua).
		rag:SetNW2Vector("DMS_PlayerColor", ent:GetPlayerColor())
		rag:SetNW2Entity("DMS_Owner", ent)
	end
	local vel = ent:GetVelocity()
	for i = 0, rag:GetPhysicsObjectCount() - 1 do
		local p = rag:GetPhysicsObjectNum(i)
		local b = ent:LookupBone(rag:GetBoneName(rag:TranslatePhysBoneToBone(i)))
		local m = b and ent:GetBoneMatrix(b)
		if IsValid(p) and m then
			p:SetPos(m:GetTranslation(), true)
			p:SetAngles(m:GetAngles())
			p:SetVelocity(vel)
			p:Wake()
		end
	end
	return rag
end

local function dropCorpse(ply)
	if IsValid(ply.DMS_Corpse) then ply.DMS_Corpse:Remove() end
	ply.DMS_Corpse = nil
	ply:SetNW2Entity("DMS_Corpse", NULL)
end

-- After the gamemode's DoPlayerDeath (its CreateRagdoll): GMod's client ragdoll gives way to a server one with NM.
hook.Add("PlayerDeath", "DMS_Deaths", function(ply)
	-- Died as a ragdoll (live.lua's drunk): that ragdoll is their corpse (DMS_DiedAsRagdoll).
	if ply.DMS_Live then return end
	-- One death ragdoll a player, as GMod's.
	dropCorpse(ply)
	if not players:GetBool() or not possible() or not DMS_HasValveBiped(ply) then return end
	local h = ply.DMS_LastHit
	local rag = DMS_RagdollFrom(ply)
	if not rag then return end
	local id, why = attach(rag, h, ply)
	if not id then
		rag:Remove()
		if why then print("[DMS] NM isn't on " .. tostring(ply) .. "'s death ragdoll: " .. tostring(why)) end
		return
	end
	-- Made in this tick: removed before the clients hear of it.
	local theirs = ply:GetRagdollEntity()
	if IsValid(theirs) then theirs:Remove() end
	follow(ply, rag)
end)

-- An NPC or a player that died as a ragdoll with NM (live.lua): that ragdoll is their death ragdoll, NM kept. The hit
-- that killed it went to it as any hit does (reactions.lua): the room's fatally injured body takes it (a death without a
-- hit, its death at once). True, or false and why (death ragdolls off for it, no room).
function DMS_DiedAsRagdoll(rag, owner)
	if not IsValid(rag) or not rag.DMS_Agent then return false, "no NM on it" end
	if not wanted(owner) or not possible() then return false, "death ragdolls are off" end
	local h = owner.DMS_LastHit
	local lines = { (h and CurTime() - h.time <= HIT_AGE) and ("health value=" .. FATALLY_INJURED) or "health value=0" }
	local ply = owner:IsPlayer() and owner or nil
	-- One death ragdoll a player: the last one goes.
	if ply then dropCorpse(ply) end
	local id, why = attach(rag, nil, ply, lines)
	if not id then return false, why end
	if ply then follow(ply, rag) end
	return true
end

hook.Add("PlayerSpawn", "DMS_Deaths", function(ply) ply:SetNW2Entity("DMS_Corpse", NULL) end)

hook.Add("PlayerDisconnected", "DMS_Deaths", dropCorpse)

-- A model's first NM body makes its 21 hulls in the fork (~0.9 s at once): for the NPCs and players that may die,
-- their model is fitted and its hulls made ahead, a part a tick (DMS.RagdollWarm). Model -> the next part, or true.
local warm, queue = {}, {}

local function warmUp(ent)
	if not IsValid(ent) or not wanted(ent) or not possible() or not DMS_HasValveBiped(ent) then return end
	local model = ent:GetModel()
	if warm[model] then return end
	warm[model] = DMS_RagdollFit(model) and 0 or true
	if warm[model] ~= true then queue[#queue + 1] = model end
end

hook.Add("Tick", "DMS_DeathsWarm", function()
	local model = queue[1]
	if not model then return end
	local ok, ms = pcall(DMS.RagdollWarm, model, game.GetWorld():GetPhysicsObject(), warm[model])
	if ok and ms then
		warm[model] = warm[model] + 1
	else
		warm[model] = true
		table.remove(queue, 1)
	end
end)

hook.Add("OnEntityCreated", "DMS_DeathsWarm", function(ent)
	if ent:IsNPC() then timer.Simple(0, function() warmUp(ent) end) end
end)
hook.Add("PlayerSpawn", "DMS_DeathsWarm", function(ply) timer.Simple(0, function() warmUp(ply) end) end)

-- dms_autoattach_time: NM off the death ragdolls that have had it that long.
timer.Create("DMS_Deaths", 1, 0, function()
	prune()
	local keep = keepFor:GetFloat()
	if keep <= 0 then return end
	for i = #corpses, 1, -1 do
		if CurTime() - corpses[i].since >= keep then release(table.remove(corpses, i)) end
	end
end)

-- The death ragdolls with NM, oldest first (dms_status, the bench).
function DMS_DeathRagdolls()
	prune()
	local out = {}
	for _, c in ipairs(corpses) do out[#out + 1] = c.ent end
	return out
end
