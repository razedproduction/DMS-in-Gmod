-- The spawn menu's "DMS" tab (ROADMAP M6, docs/adr/0009): Homer, the Playroom's reactions and switches, and the
-- player's tuning of NM's behaviours. Everything that changes the server goes through net messages (tuning.lua) and
-- only works for superadmins. The behaviours and their params come from NM itself (`nm_message_*`), so new ones
-- appear without changes here; params the port takes but doesn't use are marked.

local catalogue
local editing = { name = "Playroom", profile = { messages = {}, taskparams = {} } }
local panels = {}

local function request(t)
	local s = util.Compress(util.TableToJSON(t))
	net.Start("dms_tuning")
	net.WriteUInt(#s, 32)
	net.WriteData(s, #s)
	net.SendToServer()
end

-- The Homer the player looks at, for trying things out.
local function aimed()
	local tr = LocalPlayer():GetEyeTrace()
	local ent = tr.Entity
	if IsValid(ent) and ent:GetNW2Bool("DMS_Homer") then return ent end
	for _, e in ipairs(ents.FindByClass("prop_ragdoll")) do
		if e:GetNW2Bool("DMS_Homer") then return e end
	end
end

local function react(line)
	local ent = aimed()
	if not ent then return notification.AddLegacy("DMS: no Homer with NM", NOTIFY_ERROR, 3) end
	request({ kind = "react", ent = ent:EntIndex(), react = line })
end

local function send(line)
	local ent = aimed()
	if not ent then return notification.AddLegacy("DMS: no Homer with NM", NOTIFY_ERROR, 3) end
	request({ kind = "react", ent = ent:EntIndex(), line = line })
end

-- A range from a param's doc ("[0, 1]", "0..1", "0 to 1"), or one round its default.
local function range(p)
	local doc = p.doc or ""
	local a, b = doc:match("%[%s*(%-?[%d%.]+)%s*,%s*(%-?[%d%.]+)%s*%]")
	if not a then a, b = doc:match("(%-?[%d%.]+)%s*%.%.%s*(%-?[%d%.]+)") end
	if not a then a, b = doc:match("(%-?[%d%.]+)%s+to%s+(%-?[%d%.]+)") end
	a, b = tonumber(a), tonumber(b)
	if a and b and b > a then return a, b end
	local d = tonumber(p.default) or 0
	local m = math.max(math.abs(d) * 4, 1)
	return math.min(d - m, 0), d + m
end

local function value(msg, p)
	local ps = editing.profile.messages[msg]
	return ps and ps[p.name]
end

local function setValue(msg, name, v)
	local m = editing.profile.messages
	m[msg] = m[msg] or {}
	m[msg][name] = v
	if next(m[msg]) == nil then m[msg] = nil end
end

local function vecText(v)
	if istable(v) then return string.format("%g,%g,%g", v[1] or v.x or 0, v[2] or v.y or 0, v[3] or v.z or 0) end
	if isvector(v) then return string.format("%g,%g,%g", v.x, v.y, v.z) end
	return tostring(v or "")
end

-- One behaviour's params: a slider, a checkbox or a text box each, its doc as the tooltip.
local function fillParams(list, msg)
	list:Clear()
	local entry
	for _, m in ipairs(catalogue.messages) do if m.name == msg then entry = m end end
	if not entry then return end
	local head = list:Add("DLabel")
	head:Dock(TOP)
	head:SetText(msg .. ": " .. #entry.params .. " params (* the port takes it but doesn't use it)")
	head:SetDark(true)
	for _, p in ipairs(entry.params) do
		local row = list:Add("DPanel")
		row:Dock(TOP)
		row:SetTall(24)
		row:DockMargin(0, 1, 0, 1)
		row:SetTooltip(p.doc ~= "" and p.doc or p.name)
		local label = (p.used and "" or "* ") .. p.name
		local cur = value(msg, p)
		local reset = vgui.Create("DButton", row)
		reset:Dock(RIGHT)
		reset:SetWide(22)
		reset:SetText("↺")
		reset:SetTooltip("Back to the Playroom's default")
		if p.kind == "bool" then
			local c = vgui.Create("DCheckBoxLabel", row)
			c:Dock(FILL)
			c:SetText(label)
			c:SetDark(true)
			c:SetChecked(cur ~= nil and (cur == "true" or cur == "1") or (tonumber(p.default) or 0) ~= 0)
			c.OnChange = function(_, v) setValue(msg, p.name, v and "true" or "false") end
			reset.DoClick = function() setValue(msg, p.name, nil) fillParams(list, msg) end
		elseif p.kind == "float" or p.kind == "int" then
			local lo, hi = range(p)
			local s = vgui.Create("DNumSlider", row)
			s:Dock(FILL)
			s:SetText(label)
			s:SetDark(true)
			s:SetMinMax(lo, hi)
			s:SetDecimals(p.kind == "int" and 0 or 3)
			s:SetValue(tonumber(cur) or tonumber(p.default) or 0)
			s.OnValueChanged = function(_, v)
				setValue(msg, p.name, p.kind == "int" and tostring(math.Round(v)) or string.format("%.4g", v))
			end
			reset.DoClick = function() setValue(msg, p.name, nil) fillParams(list, msg) end
		else
			local l = vgui.Create("DLabel", row)
			l:Dock(LEFT)
			l:SetWide(170)
			l:SetText(label)
			l:SetDark(true)
			local t = vgui.Create("DTextEntry", row)
			t:Dock(FILL)
			t:SetValue(cur or vecText(p.default))
			t.OnEnter = function(self)
				local v = self:GetValue():gsub("%s", "")
				setValue(msg, p.name, v ~= "" and v or nil)
			end
			t.OnLoseFocus = t.OnEnter
			reset.DoClick = function() setValue(msg, p.name, nil) fillParams(list, msg) end
		end
	end
end

-- The game's taskparams sets (the tasks' tuning), editable value by value.
local function fillTaskParams(list)
	list:Clear()
	local tp = catalogue.taskparams or {}
	if next(tp) == nil then
		local l = list:Add("DLabel")
		l:Dock(TOP)
		l:SetDark(true)
		l:SetWrap(true)
		l:SetAutoStretchVertical(true)
		l:SetText("This game has no taskparams.txt (the PC version): NM's tasks run their defaults, nothing to tune here.")
		return
	end
	for task, sets in SortedPairs(tp) do
		local cat = list:Add("DCollapsibleCategory")
		cat:Dock(TOP)
		cat:SetLabel(task)
		cat:SetExpanded(false)
		local inner = vgui.Create("DListLayout")
		cat:SetContents(inner)
		for set, params in SortedPairs(sets) do
			for p, v in SortedPairs(params) do
				local key = task .. " " .. set .. " " .. p
				local s = vgui.Create("DNumSlider")
				s:SetText(set .. " " .. p)
				s:SetDark(true)
				local m = math.max(math.abs(v) * 4, 1)
				s:SetMinMax(math.min(v - m, 0), v + m)
				s:SetDecimals(3)
				s:SetValue(editing.profile.taskparams[key] or v)
				s.OnValueChanged = function(_, x)
					editing.profile.taskparams[key] = math.abs(x - v) > 1e-6 and x or nil
				end
				inner:Add(s)
			end
		end
	end
end

-- What the Playroom does on each reaction (module/src/follow/room.rs), with a button to try it on the Homer looked at.
local REACTIONS = {
	{ "Shot (pistol)", "shot weapon=pistol", "applyImpulse + the game's ShotTask, the weapon's taskparams set (weapon_set)" },
	{ "Shot (shotgun)", "shot weapon=shotgun", "the same, the shotgun's set; pellets add their damage" },
	{ "Shot (sniper)", "shot weapon=sniper", "the same, the sniper's set" },
	{ "Blast", "blast", "applyImpulse on each part within 5 m, 250 damage at 0 m, the explosion task (6 s)" },
	{ "Fire", "burn", "25 damage a second, NM's OnFire task (fire_reaction)" },
	{ "Taser", "taser", "electrocute stunMag=0.4 largeMult=2 for 2.5 s" },
	{ "Blow", "melee damage=10", "applyImpulse, the game's melee task: flinch (MELEE / MELEE_PASIVE) or balance" },
	{ "Near miss", "nearmiss", "bodyBalance, upperBodyFlinch and headLook at the bullet (1.5 m)" },
	{ "Car", "car", "the game's bump damage and its task: brace, rollup, balance or relax" },
	{ "Thrown at", "thrown", "headLook (look), upperBodyFlinch (avoid)" },
	{ "Kill", "kill", "health 0: stopAllBehaviours, bodyRelax" },
	{ "Health back", "health value=200", "health 200, alive" },
}

local function build(panel)
	panel:ClearControls()
	panels.root = panel
	panel:Help("Delirium on Homer (models/dms/homer.mdl). Changing anything here needs superadmin.")
	panel:Button("Spawn Homer", "dms_homer")
	panel:Button("NM off every Homer", "dms_detach_all")
	panel:CheckBox("Eyes", "dms_eyes")
	panel:CheckBox("Eyebrows", "dms_brows")
	panel:CheckBox("Skeleton", "dms_skeleton")
	panel:CheckBox("Debug overlay: NM's capsules, drives, centre of mass", "dms_overlay")
	local col = panel:ComboBox("Colours", "dms_pedcol")
	for i, n in ipairs({ "Red & blue", "All blue", "All red", "Solid blue", "Opaque red & blue" }) do col:AddChoice(n, i - 1) end
	col.OnSelect = function(_, _, _, v) request({ kind = "switch", name = "pedcol", value = v }) end

	panel:Help("The Playroom's reactions (its defaults are the Playroom's; golden runs ignore these):")
	local switches = {
		{ "tasks", "NM -> animation as the game's tasks end" }, { "look", "Looks at what's thrown at him" },
		{ "avoid", "Flinches from what's thrown at him" }, { "fire_reaction", "OnFire when he burns" },
		{ "dive_shot", "Shot at again, he dives" }, { "extra_reactions", "Additional reactions" },
		{ "weak", "Weaker weapon sets" }, { "god", "Infinite health" },
	}
	for _, s in ipairs(switches) do
		local c = panel:CheckBox(s[2], "dms_" .. s[1])
		c.OnChange = function(_, v) request({ kind = "switch", name = s[1], value = v and 1 or 0 }) end
	end
	local gu = panel:ComboBox("Get-up", "dms_getup")
	for i, n in ipairs({ "Animation (the game's clip)", "Muscles", "Off", "Learned (not ported)" }) do gu:AddChoice(n, i - 1) end
	gu.OnSelect = function(_, _, _, v) request({ kind = "switch", name = "getup", value = v }) end

	panel:Help("Try a reaction on the Homer you look at:")
	for _, r in ipairs(REACTIONS) do
		local b = panel:Button(r[1])
		b:SetTooltip(r[3])
		b.DoClick = function() react(r[2]) end
	end

	panel:Help("Tuning (docs/adr/0009): only through NM's own inputs, off = the Playroom's.")
	local on = panel:CheckBox("Tuning on", "dms_tuning")
	local prof = vgui.Create("DComboBox")
	panel:AddItem(prof)
	panels.profile = prof
	on.OnChange = function(_, v) request({ kind = "use", name = editing.name, on = v }) end
	prof.OnSelect = function(_, _, name)
		request({ kind = "profile", name = name })
		request({ kind = "use", name = name, on = GetConVar("dms_tuning"):GetBool() })
	end
	local nameEntry = vgui.Create("DTextEntry")
	nameEntry:SetPlaceholderText("profile name")
	panel:AddItem(nameEntry)
	local save = panel:Button("Save profile")
	save.DoClick = function()
		local name = nameEntry:GetValue() ~= "" and nameEntry:GetValue() or editing.name
		if name == "Playroom" then return notification.AddLegacy("DMS: the Playroom profile can't be changed: give a name", NOTIFY_ERROR, 4) end
		editing.name = name
		request({ kind = "save", name = name, profile = editing.profile })
	end
	panel:Button("Delete profile").DoClick = function() request({ kind = "delete", name = editing.name }) end
	panel:Button("Reset to Playroom").DoClick = function()
		editing = { name = "Playroom", profile = { messages = {}, taskparams = {} } }
		request({ kind = "reset" })
	end

	local behaviours = vgui.Create("DListView")
	behaviours:SetTall(180)
	behaviours:AddColumn("Behaviour (NM's messages)")
	behaviours:SetMultiSelect(false)
	panel:AddItem(behaviours)
	panels.behaviours = behaviours
	local params = vgui.Create("DScrollPanel")
	params:SetTall(320)
	panel:AddItem(params)
	panels.params = params
	behaviours.OnRowSelected = function(_, _, row) panels.msg = row:GetColumnText(1) fillParams(params, panels.msg) end
	local try = panel:Button("Start it on Homer (with these values)")
	try:SetTooltip("Sends `<behaviour> start=true` plus the edited values to the Homer you look at")
	try.DoClick = function()
		if not panels.msg then return end
		local kv = { panels.msg, "start=true" }
		for k, v in SortedPairs(editing.profile.messages[panels.msg] or {}) do kv[#kv + 1] = k .. "=" .. v end
		send(table.concat(kv, " "))
	end
	panel:Help("The game's taskparams (the tasks' sets: shot, melee, explosion, fire, high fall, ...):")
	local tp = vgui.Create("DScrollPanel")
	tp:SetTall(260)
	panel:AddItem(tp)
	panels.tp = tp
	request({ kind = "catalogue" })
end

local function refresh()
	if not catalogue or not IsValid(panels.behaviours) then return end
	panels.behaviours:Clear()
	for _, m in ipairs(catalogue.messages) do panels.behaviours:AddLine(m.name) end
	panels.profile:Clear()
	for _, n in ipairs(catalogue.profiles) do panels.profile:AddChoice(n, nil, n == catalogue.profile) end
	fillTaskParams(panels.tp)
	if panels.msg then fillParams(panels.params, panels.msg) end
end

net.Receive("dms_tuning", function()
	local n = net.ReadUInt(32)
	local r = util.JSONToTable(util.Decompress(net.ReadData(n)) or "")
	if not r then return end
	if r.kind == "catalogue" then
		catalogue = r.data
		if editing.name ~= catalogue.profile then request({ kind = "profile", name = catalogue.profile }) end
		refresh()
	elseif r.kind == "profile" then
		editing = { name = r.data.name, profile = r.data.profile }
		editing.profile.messages = editing.profile.messages or {}
		editing.profile.taskparams = editing.profile.taskparams or {}
		refresh()
	elseif r.kind == "error" then
		notification.AddLegacy("DMS: " .. tostring(r.data), NOTIFY_ERROR, 4)
	end
end)

hook.Add("PopulateToolMenu", "DMS_Menu", function()
	spawnmenu.AddToolMenuOption("Utilities", "DMS", "DMS_Homer", "Homer (Delirium)", "", "", build)
end)

hook.Add("AddToolMenuCategories", "DMS_Menu", function()
	spawnmenu.AddToolCategory("Utilities", "DMS", "DMS")
end)
