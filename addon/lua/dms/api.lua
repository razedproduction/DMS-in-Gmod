-- The public Lua API (ROADMAP M6, docs/api.md): `dms.Attach(rag)` puts NM on a Homer ragdoll and gives a controller
-- whose methods send NM's messages or the Playroom's reactions; hooks DMS_Attached, DMS_Detached, DMS_Fallen,
-- DMS_Feedback, DMS_Died, DMS_Reaction tell what happened (from the module's event queue, docs/adr/0003).

dms = dms or {}

local M = 0.0254

local Controller = {}
Controller.__index = Controller

local function at(p) return string.format("%.4f,%.4f,%.4f", p.x * M, p.y * M, p.z * M) end

-- A Homer ragdoll's controller, or nil if NM isn't on it.
function dms.Get(rag)
	if not IsValid(rag) or not rag.DMS_Agent then return nil end
	return setmetatable({ ent = rag }, Controller)
end

-- NM on a Homer ragdoll (models/dms/homer.mdl, bound at its spawn). opts: `balance` (default true: bodyBalance, as
-- the Playroom's "balance" button). The controller, or nil and why.
function dms.Attach(rag, opts)
	opts = opts or {}
	if not IsValid(rag) or rag:GetModel() ~= DMS_HOMER_MODEL then return nil, "not a Homer ragdoll (" .. DMS_HOMER_MODEL .. ")" end
	if not rag.DMS_Bound then return nil, "not bound to NM's body (spawn him with dms_homer or the spawn menu)" end
	local id, why = DMS_Follow(rag)
	if not id then return nil, why end
	local c = dms.Get(rag)
	if opts.balance ~= false then c:Balance() end
	return c
end

-- NM off: an ordinary ragdoll again.
function dms.Detach(rag)
	if IsValid(rag) and rag.DMS_Agent then DMS_Unfollow(rag) return true end
	return false
end

-- Every Homer with NM on him.
function dms.All()
	local out = {}
	for _, ent in pairs(DMS_Agents) do
		if IsValid(ent) then out[#out + 1] = dms.Get(ent) end
	end
	return out
end

-- NM's messages (`DMS.Messages`) and a message's params (`DMS.MessageParams`), for introspection.
function dms.Messages() return DMS.Messages() end
function dms.MessageParams(name) return DMS.MessageParams(name) end

-- The Playroom's reactions, buttons, additional reactions, get-up modes and weapons (`DMS.Reactions`).
function dms.Reactions() return DMS.Reactions() end

function Controller:IsValid() return IsValid(self.ent) and self.ent.DMS_Agent ~= nil end
function Controller:Entity() return self.ent end
function Controller:Id() return self.ent.DMS_Agent end

-- A message line as the Playroom's console takes it: `ctrl:Send("bodyBalance", {start = true})` or
-- `ctrl:Send("bodyBalance start=true")`. Vectors (Vector) are points in the game's frame (moved into NM's) unless
-- the param is a direction; give a "x,y,z" string to pass NM's own frame through.
local DIRECTIONS = { dir = true, normal = true, bulletVel = true, impulse = true, vel = true, fallOverEdge = true }
function Controller:Send(msg, params)
	if not self:IsValid() then return false, "no NM on it" end
	local line = msg
	if params then
		local origin = self:Origin()
		local kv = {}
		for k, v in SortedPairs(params) do
			if isvector(v) then
				local p = DIRECTIONS[k] and v or (v * M - origin)
				v = string.format("%.4f,%.4f,%.4f", p.x, p.y, p.z)
			elseif isbool(v) then
				v = v and "true" or "false"
			end
			kv[#kv + 1] = k .. "=" .. tostring(v)
		end
		if #kv > 0 then line = line .. " " .. table.concat(kv, " ") end
	end
	return DMS_Send(self.ent, line)
end

-- Where NM's frame starts (m, the game's frame): its messages' points are from there.
function Controller:Origin()
	if not self.ent.DMS_Origin then
		local s = self.ent.DMS_Spawn
		self.ent.DMS_Origin = s and s.pos * M or Vector()
	end
	return self.ent.DMS_Origin
end

-- A reaction of the Playroom's (`DMS.FollowReact`'s line, points in the game's frame, m; dms_react).
function Controller:React(line) return DMS_React(self.ent, line) end

-- A bullet: `where` a part (0-20) or a point (Vector), `dir` the bullet's way (Vector), `weapon` the Playroom's
-- (pistol, deagle, smg, ak47, sniper, shotgun, ...).
function Controller:Shot(where, dir, weapon)
	local line = "shot weapon=" .. (weapon or "pistol")
	if isnumber(where) then line = line .. " part=" .. where end
	if isvector(where) then
		dir = dir or (where - self.ent:WorldSpaceCenter()):GetNormalized() * -1
		line = line .. string.format(" at=%s dir=%.4f,%.4f,%.4f", at(where), dir.x, dir.y, dir.z)
	end
	return self:React(line)
end

-- The Playroom's buttons.
function Controller:Button(name) return self:React("button name=" .. name:gsub(" ", "_")) end
function Controller:Balance() return self:Button("balance") end
function Controller:CatchFall() return self:Button("catchfall") end
function Controller:Relax() return self:Button("relax") end
function Controller:Stop() return self:Button("STOP all") end

-- A flinch: from a point (Vector) as the game's flinch task (upperBodyFlinch), or the Playroom's button.
function Controller:Flinch(from)
	if isvector(from) then return self:Send("upperBodyFlinch", { start = true, pos = from }) end
	return self:Button("flinch")
end

function Controller:Kill() return self:React("kill") end
function Controller:SetHealth(h) return self:React("health value=" .. h) end

-- NM's state: `{steps, awake, asleep, balancing, fallen, pelvis, head, com, com_v (m), health, dead, rising, burning,
-- shock, getup}`.
function Controller:GetState()
	local id = self.ent.DMS_Agent
	local s = id and DMS.FollowState(id)
	if not s then return nil end
	local r = DMS.FollowRoom(id) or {}
	for _, k in ipairs({ "health", "dead", "rising", "burning", "shock", "getup" }) do s[k] = r[k] end
	return s
end

-- A message param's default and doc (`DMS.MessageParams`): `{name, kind, doc, default, used, min, max}`.
function Controller:GetParam(msg, param)
	for _, p in ipairs(DMS.MessageParams(msg) or {}) do
		if p.name == param then return p end
	end
end

-- Homers spawned from the spawn menu are bound in their spawn pose (DMS.BindHomer) and, with dms_autoattach, get NM.
local autoattach = CreateConVar("dms_autoattach", "1", FCVAR_ARCHIVE, "NM on Homers spawned from the spawn menu")
hook.Add("PlayerSpawnedRagdoll", "DMS_Bind", function(ply, model, ent)
	if model ~= DMS_HOMER_MODEL or ent.DMS_Bound then return end
	local ok, why = DMS_BindRagdoll(ent)
	if not ok then return DMS_Say(ply, "[DMS] Homer isn't bound: " .. tostring(why)) end
	if autoattach:GetBool() then
		local c, err = dms.Attach(ent)
		if not c then DMS_Say(ply, "[DMS] NM isn't on Homer: " .. tostring(err)) end
	end
end)
