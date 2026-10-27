-- What the clients draw over Homer (ROADMAP M6, cl_face.lua, cl_overlay.lua), the server's side: his parts' NM
-- shapes in their bones' frames (once), NM's view of him (drives, contacts, centre of mass, balance) 15 times a
-- second to the superadmins who turned the debug overlay on, and the Playroom's colour schemes as the model's skins.

local M = 0.0254
local N = 25 -- numbers a part in DMS.FollowOverlay

util.AddNetworkString("dms_overlay")
util.AddNetworkString("dms_shape")

-- The Playroom's colour schemes (`pedcol`): the model's skins 0 mixed, 1 all blue, 2 all red, 3 solid blue, 4 mixed
-- opaque (tools/dms-homer).
local pedcol = CreateConVar("dms_pedcol", "0", { FCVAR_ARCHIVE, FCVAR_REPLICATED }, "Homer's colours: 0 red & blue, 1 all blue, 2 all red, 3 solid blue, 4 opaque red & blue")
local function skins()
	for _, ent in pairs(DMS_Agents) do
		if IsValid(ent) then ent:SetSkin(math.Clamp(pedcol:GetInt(), 0, 4)) end
	end
end
cvars.AddChangeCallback("dms_pedcol", skins, "DMS_Overlay")
hook.Add("DMS_Attached", "DMS_Overlay", function(ent) ent:SetSkin(math.Clamp(pedcol:GetInt(), 0, 4)) end)

-- Each part's NM shape in its bone's frame (units): capsule ends, radius, box half sizes, the box's angles, the part's
-- centre; and its parent. The same for every Homer, taken from one with NM on him.
local shape
local function makeShape(ent)
	local parts = DMS_HomerParts(ent)
	local o = parts and DMS.FollowOverlay(ent.DMS_Agent)
	if not o then return end
	local out = {}
	for k, p in ipairs(parts) do
		local i = (k - 1) * N
		local v = function(j) return Vector(o[i + j], o[i + j + 1], o[i + j + 2]) end
		local q = { o[i + 11], o[i + 12], o[i + 13], o[i + 14] }
		-- The capsule's rotation as a direction pair (its Y and Z), in the bone's frame.
		local function rot(x, y, z)
			local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
			local tx, ty, tz = 2 * (qy * z - qz * y), 2 * (qz * x - qx * z), 2 * (qx * y - qy * x)
			return Vector(x + qw * tx + (qy * tz - qz * ty), y + qw * ty + (qz * tx - qx * tz), z + qw * tz + (qx * ty - qy * tx))
		end
		local pos, ang = p:GetPos(), p:GetAngles()
		local loc = function(w) return WorldToLocal(w, angle_zero, pos, ang) end
		local dirLoc = function(w) return loc(pos + w) end
		out[k] = {
			a = loc(v(1)), b = loc(v(4)), r = o[i + 7], box = v(8),
			x = dirLoc(rot(1, 0, 0)), y = dirLoc(rot(0, 1, 0)), z = dirLoc(rot(0, 0, 1)),
			centre = p:GetMassCenter(), parent = o[i + 18],
			bone = ent:TranslatePhysBoneToBone(k - 1), name = DMS.PartNames()[k],
		}
	end
	return out
end

net.Receive("dms_shape", function(_, ply)
	if not shape then
		for _, ent in pairs(DMS_Agents) do
			if IsValid(ent) then shape = makeShape(ent) break end
		end
	end
	if not shape then return end
	local s = util.Compress(util.TableToJSON(shape))
	net.Start("dms_shape")
	net.WriteUInt(#s, 32)
	net.WriteData(s, #s)
	net.Send(ply)
end)

-- Superadmins who want the debug overlay (cl_overlay.lua's dms_overlay).
local watchers = {}
net.Receive("dms_overlay", function(_, ply)
	if ply:IsSuperAdmin() then watchers[ply] = net.ReadBool() or nil end
end)

local nextSend = 0
hook.Add("Think", "DMS_Overlay", function()
	if CurTime() < nextSend or not next(watchers) then return end
	nextSend = CurTime() + 1 / 15
	local to = {}
	for ply in pairs(watchers) do
		if IsValid(ply) then to[#to + 1] = ply else watchers[ply] = nil end
	end
	for _, ent in pairs(DMS_Agents) do
		local o = IsValid(ent) and DMS.FollowOverlay(ent.DMS_Agent)
		if o then
			net.Start("dms_overlay", true)
			net.WriteEntity(ent)
			net.WriteUInt(#o, 16)
			for _, x in ipairs(o) do net.WriteFloat(x) end
			net.Send(to)
		end
	end
end)

DMS_OVERLAY_PART = N
DMS_OVERLAY_M = M
