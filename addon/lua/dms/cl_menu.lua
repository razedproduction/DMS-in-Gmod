-- The DMS window (docs/adr/0022): its own frame over the game with Homer, the Playroom's reactions and switches, the
-- player's tuning of NM's behaviours (docs/adr/0009) and the agents' limits (docs/adr/0021). `dms_menu` opens it, and
-- so do a key (dms_menu_key), the context menu's icon and the spawn menu's "Utilities -> DMS". Tabs on the left; what's
-- under the cursor is explained on the right. Everything that changes the server goes through net messages
-- (tuning.lua) and only works for superadmins. The behaviours and their params come from NM itself (`nm_message_*`),
-- so new ones appear without changes here; params the port takes but doesn't use are marked.

local menuKey = CreateClientConVar("dms_menu_key", "0", true, false, "A key that opens and closes the DMS window (0: none)")

local catalogue
local paramsOf = {} -- message -> its params, as the server sent them
local asked = {} -- messages whose params are on their way
local editing = { name = "Playroom", profile = { messages = {}, taskparams = {} } }
local ui = {} -- the open window's panels and setters; a new one with each window
local lastTab, lastMsg = "Homer", nil
-- While above 0 the window fills its own widgets: their callbacks aren't the player's (a profile picked as the list
-- filled looped with the server and wiped the edits).
local filling = 0

local function quietly(f)
	filling = filling + 1
	local ok, err = pcall(f)
	filling = filling - 1
	if not ok then ErrorNoHaltWithStack(err) end
end

local function request(t)
	local s = util.Compress(util.TableToJSON(t))
	net.Start("dms_tuning")
	net.WriteUInt(#s, 32)
	net.WriteData(s, #s)
	net.SendToServer()
end

-- A server convar (tuning.lua's SWITCHES); sliders send theirs once they stop moving.
local function switch(name, value, later)
	if not later then return request({ kind = "switch", name = name, value = value }) end
	timer.Create("DMS_Switch_" .. name, 0.3, 1, function() request({ kind = "switch", name = name, value = value }) end)
end

-- The Homer the player looks at, for trying things out.
local function aimed()
	local tr = LocalPlayer():GetEyeTrace()
	local ent = tr.Entity
	if IsValid(ent) and ent:GetNW2Bool("DMS_Agent") then return ent end
	for _, e in ipairs(ents.FindByClass("prop_ragdoll")) do
		if e:GetNW2Bool("DMS_Agent") then return e end
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

-- The look: dark glass over the blurred game, one warm accent.
local C = {
	bg = Color(18, 20, 24, 235),
	pane = Color(255, 255, 255, 8),
	hover = Color(255, 255, 255, 22),
	line = Color(255, 255, 255, 36),
	accent = Color(255, 196, 70),
	text = Color(232, 234, 238),
	dim = Color(150, 156, 166),
	bad = Color(255, 120, 90),
}
surface.CreateFont("DMS_Title", { font = "Roboto", size = 22, weight = 700, extended = true })
surface.CreateFont("DMS_Head", { font = "Roboto", size = 17, weight = 700, extended = true })
surface.CreateFont("DMS_Text", { font = "Roboto", size = 15, weight = 500, extended = true })

local function paintPane(_, w, h)
	surface.SetDrawColor(C.pane)
	surface.DrawRect(0, 0, w, h)
	surface.SetDrawColor(C.line)
	surface.DrawOutlinedRect(0, 0, w, h)
end

local function styleScroll(sp)
	local bar = sp:GetVBar()
	bar:SetWide(8)
	bar:SetHideButtons(true)
	bar.Paint = nil
	bar.btnGrip.Paint = function(_, w, h) draw.RoundedBox(4, 1, 0, w - 2, h, C.line) end
	return sp
end

-- What the help pane says while the cursor is over `p` (or anything inside it); `text` may be a function.
local function help(p, title, text)
	p.dmsHelp = { title, text or "" }
	return p
end

local function heading(parent, text)
	local l = parent:Add("DLabel")
	l:Dock(TOP)
	l:DockMargin(0, 10, 0, 4)
	l:SetFont("DMS_Head")
	l:SetTextColor(C.accent)
	l:SetText(text)
	l:SizeToContentsY(6)
	l.Paint = function(_, w, h)
		surface.SetDrawColor(C.line)
		surface.DrawLine(0, h - 1, w, h - 1)
	end
	return l
end

local function note(parent, text, color)
	local l = parent:Add("DLabel")
	l:Dock(TOP)
	l:DockMargin(0, 2, 0, 2)
	l:SetFont("DMS_Text")
	l:SetTextColor(color or C.dim)
	l:SetWrap(true)
	l:SetAutoStretchVertical(true)
	l:SetText(text)
	return l
end

local function paintButton(self, w, h)
	local on = self:IsEnabled() and (self:IsHovered() or self.Depressed or self.dmsActive)
	surface.SetDrawColor(on and C.hover or C.pane)
	surface.DrawRect(0, 0, w, h)
	surface.SetDrawColor(self.dmsActive and C.accent or C.line)
	surface.DrawOutlinedRect(0, 0, w, h)
end

local function button(parent, text, helpText, fn)
	local b = parent:Add("DButton")
	b:SetTall(26)
	b:SetFont("DMS_Text")
	b:SetTextColor(C.text)
	b:SetText(text)
	b.Paint = paintButton
	b.DoClick = function() fn() end
	if helpText then help(b, text, helpText) end
	return b
end

local function check(parent, text, helpText, onChange)
	local c = parent:Add("DCheckBoxLabel")
	c:Dock(TOP)
	c:DockMargin(0, 3, 0, 3)
	c:SetFont("DMS_Text")
	c:SetTextColor(C.text)
	c:SetText(text)
	if onChange then c.OnChange = function(_, v) if filling == 0 then onChange(v) end end end
	help(c, text, helpText)
	return c
end

local function slider(parent, text, helpText, lo, hi, decimals, onChange)
	local s = parent:Add("DNumSlider")
	s:Dock(TOP)
	s:SetTall(26)
	s:SetText(text)
	s:SetMinMax(lo, hi)
	s:SetDecimals(decimals)
	s.Label:SetFont("DMS_Text")
	s.Label:SetTextColor(C.text)
	s.TextArea:SetTextColor(C.text)
	s.OnValueChanged = function(_, v) if filling == 0 then onChange(v) end end
	help(s, text, helpText)
	return s
end

local function label(parent, text, wide)
	local l = parent:Add("DLabel")
	l:Dock(LEFT)
	l:SetWide(wide)
	l:SetFont("DMS_Text")
	l:SetTextColor(C.text)
	l:SetText(text)
	return l
end

local function row(parent, tall)
	local r = parent:Add("DPanel")
	r:Dock(TOP)
	r:DockMargin(0, 3, 0, 3)
	r:SetTall(tall or 26)
	r.Paint = nil
	return r
end

local function combo(parent, text, helpText, choices, onSelect)
	local r = row(parent, 24)
	label(r, text, 160)
	local cb = r:Add("DComboBox")
	cb:Dock(LEFT)
	cb:SetWide(240)
	cb:SetSortItems(false)
	for i, n in ipairs(choices) do cb:AddChoice(n, i - 1) end
	cb.OnSelect = function(_, _, _, v) if filling == 0 then onSelect(v) end end
	help(r, text, helpText)
	return cb
end

local function page(parent)
	local sp = styleScroll(parent:Add("DScrollPanel"))
	sp:Dock(FILL)
	sp:GetCanvas():DockPadding(4, 0, 12, 8)
	return sp
end

-- The server's convars in the window: their help texts and values come from the catalogue and the switches' replies.
local function switchHelp(name)
	return function()
		local s = catalogue and catalogue.switches and catalogue.switches[name]
		return (s and s.help or "(the server hasn't answered yet)") .. "\n\ndms_" .. name
	end
end

local function bindSwitch(name, set) ui.switch[name] = set end

local function applySwitches(values)
	if not values or not ui.switch then return end
	quietly(function()
		for name, set in pairs(ui.switch) do
			local s = values[name]
			if s then set(s.value) end
		end
	end)
end

-- Tabs ----------------------------------------------------------------------------------------------------------

local function homerTab(p)
	heading(p, "Homer")
	note(p, "Delirium on Homer (models/dms/homer.mdl). Changing the server needs superadmin.")
	local r = row(p)
	local spawn = button(r, "Spawn Homer", "A Homer in front of you with NM on him, balancing.\n\ndms_homer", function() RunConsoleCommand("dms_homer") end)
	spawn:Dock(LEFT)
	spawn:SetWide(180)
	local off = button(r, "NM off every Homer", "Takes NM off every Homer: they become ordinary ragdolls.\n\ndms_detach_all", function() RunConsoleCommand("dms_detach_all") end)
	off:Dock(LEFT)
	off:DockMargin(6, 0, 0, 0)
	off:SetWide(180)
	local a = row(p)
	local attach = button(a, "NM on the Homer you look at", "Puts NM back on the Homer ragdoll you look at (one spawned with dms_homer, the spawn menu or the DMS tool): he balances.\n\ndms_attach", function() RunConsoleCommand("dms_attach") end)
	attach:Dock(LEFT)
	attach:SetWide(180)
	local detach = button(a, "NM off the Homer you look at", "Takes NM off the Homer you look at: an ordinary ragdoll until NM is put back on.\n\ndms_detach", function() RunConsoleCommand("dms_detach") end)
	detach:Dock(LEFT)
	detach:DockMargin(6, 0, 0, 0)
	detach:SetWide(180)
	note(p, "Close the window to aim: these act on the Homer under your crosshair. The DMS tool (Tools -> DMS) does the same and more.")
	local auto = check(p, "NM on Homers spawned from the spawn menu", switchHelp("autoattach"), function(v) switch("autoattach", v and 1 or 0) end)
	bindSwitch("autoattach", function(v) auto:SetChecked(tonumber(v) ~= 0) end)

	heading(p, "Any ragdoll (ValveBiped models)")
	note(p, "NM on GMod's own ragdolls: HL2's citizens, combine, Alyx, player models. Homer's body is fitted to the model's skeleton (sizes, masses) and NM drives it as it drives Homer; the model keeps its look. Getting up by animation is Homer's only: a fitted body stays down.")
	local g = row(p)
	local on = button(g, "NM on the ragdoll you look at", "Puts NM on the ragdoll under your crosshair, as it lies or falls: Homer, or any ValveBiped model (its fit is shown below).\n\ndms_attach", function() RunConsoleCommand("dms_attach") end)
	on:Dock(LEFT)
	on:SetWide(180)
	local offr = button(g, "NM off the ragdoll you look at", "Takes NM off: the model gets its own physics back, moving on as it was.\n\ndms_detach", function() RunConsoleCommand("dms_detach") end)
	offr:Dock(LEFT)
	offr:DockMargin(6, 0, 0, 0)
	offr:SetWide(180)
	local s = row(p)
	local citizen = button(s, "Spawn a citizen with NM", "A citizen (male_07) in front of you on a fitted body, balancing.\n\ndms_ragdoll [model]", function() RunConsoleCommand("dms_ragdoll") end)
	citizen:Dock(LEFT)
	citizen:SetWide(180)
	local fit = note(p, "")
	fit.Think = function(self)
		if (self.dmsNext or 0) > CurTime() then return end
		self.dmsNext = CurTime() + 0.3
		local ent = LocalPlayer():GetEyeTrace().Entity
		local text
		if IsValid(ent) and ent:GetNW2Bool("DMS_Ragdoll") then
			local warn = ent:GetNW2String("DMS_FitWarn")
			text = string.format("Under your crosshair: %s, fitted %.2f m, %.1f kg (Homer 1.77 m, 77.7 kg)%s", ent:GetModel(),
				ent:GetNW2Float("DMS_FitHeight"), ent:GetNW2Float("DMS_FitMass"), warn ~= "" and ("\nWarnings: " .. warn) or "")
		elseif IsValid(ent) and ent:GetClass() == "prop_ragdoll" then
			text = "Under your crosshair: " .. ent:GetModel() .. " (no NM on it: its fit shows once it has)"
		else
			text = "Look at a ragdoll (close the window to aim) to see its fit."
		end
		if text ~= self:GetText() then self:SetText(text) end
	end
	local autor = check(p, "NM on ValveBiped ragdolls spawned from the spawn menu", "They balance where they land.\n\ndms_autoattach_ragdolls")
	autor:SetConVar("dms_autoattach_ragdolls")
	for _, c in ipairs({
		{ "autoattach_npc", "NM on killed NPCs' ragdolls" },
		{ "autoattach_players", "NM on killed players' ragdolls (a server ragdoll, the death camera on it)" },
	}) do
		local box = check(p, c[2], switchHelp(c[1]), function(v) switch(c[1], v and 1 or 0) end)
		bindSwitch(c[1], function(v) box:SetChecked(tonumber(v) ~= 0) end)
	end

	heading(p, "What you see (this client)")
	for _, c in ipairs({
		{ "Eyes", "dms_eyes", "Homer's eyes, the Playroom's." },
		{ "Eyebrows", "dms_brows", "Homer's eyebrows, the Playroom's." },
		{ "Skeleton", "dms_skeleton", "The Playroom's skeleton drawn over Homer." },
		{ "Debug overlay", "dms_overlay", "NM's capsules, drives and centre of mass over Homer (superadmins)." },
	}) do
		check(p, c[1], c[3] .. "\n\n" .. c[2]):SetConVar(c[2])
	end
	local col = combo(p, "Colours", switchHelp("pedcol"),
		{ "Red & blue", "All blue", "All red", "Solid blue", "Opaque red & blue" }, function(v) switch("pedcol", v) end)
	bindSwitch("pedcol", function(v) col:ChooseOptionID((tonumber(v) or 0) + 1) end)

	heading(p, "This window")
	local k = row(p)
	label(k, "Key", 160)
	local bind = k:Add("DBinder")
	bind:Dock(LEFT)
	bind:SetWide(140)
	bind:SetConVar("dms_menu_key")
	help(k, "Key", "A key that opens and closes this window (right-click the button to clear it). The console command dms_menu does the same, as does the DMS icon in the context menu (hold C).\n\ndms_menu_key")
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

-- dms_extra_pick's bits, in order.
local EXTRAS = {
	"Windmill", "Curl up before getting up", "Curl up when shot at lying", "Look at who hit him", "Tumble off a ledge",
	"smartFall when dropped", "Dodge from above", "Brace on a wall",
}

local function reactionsTab(p)
	heading(p, "The Playroom's switches")
	note(p, "Their defaults are the Playroom's; golden runs ignore them.")
	local function toggle(name, text)
		local c = check(p, text, switchHelp(name), function(v) switch(name, v and 1 or 0) end)
		bindSwitch(name, function(v) c:SetChecked(tonumber(v) ~= 0) end)
		return c
	end
	toggle("tasks", "NM -> animation as the game's tasks end")
	toggle("look", "Looks at what's thrown at him")
	toggle("avoid", "Flinches from what's thrown at him")
	toggle("fire_reaction", "OnFire when he burns")
	toggle("dive_shot", "Shot at again, he dives")
	local gap = slider(p, "Seconds between dives", switchHelp("dodge_gap"), 0, 10, 1, function(v) switch("dodge_gap", string.format("%.1f", v), true) end)
	bindSwitch("dodge_gap", function(v) gap:SetValue(tonumber(v) or 3) end)
	toggle("weak", "Weaker weapon sets")
	toggle("god", "Infinite health")
	local gu = combo(p, "Get-up", switchHelp("getup"),
		{ "Animation (the game's clip)", "Muscles", "Off", "Learned (not ported)" }, function(v) switch("getup", v) end)
	bindSwitch("getup", function(v) gu:ChooseOptionID((tonumber(v) or 0) + 1) end)

	heading(p, "Additional reactions")
	toggle("extra_reactions", "Additional reactions on")
	local bits = {}
	local function pick()
		local n = 0
		for i, c in ipairs(bits) do if c:GetChecked() then n = n + 2 ^ (i - 1) end end
		switch("extra_pick", n)
	end
	for i, text in ipairs(EXTRAS) do
		local c = check(p, text, switchHelp("extra_pick"), pick)
		c:DockMargin(18, 2, 0, 2)
		bits[i] = c
	end
	bindSwitch("extra_pick", function(v)
		local n = tonumber(v) or 0
		for i, c in ipairs(bits) do c:SetChecked(bit.band(n, 2 ^ (i - 1)) ~= 0) end
	end)

	heading(p, "Try a reaction on the Homer you look at")
	local grid = p:Add("DIconLayout")
	grid:Dock(TOP)
	grid:SetSpaceX(6)
	grid:SetSpaceY(6)
	for _, r in ipairs(REACTIONS) do
		button(grid, r[1], r[3] .. "\n\ndms_react " .. r[2], function() react(r[2]) end):SetSize(150, 26)
	end
end

local function performanceTab(p)
	heading(p, "Agents (docs/adr/0021)")
	note(p, "How many Homers NM drives and when it lets them go. The server's; superadmins only.")
	local function num(name, text, lo, hi, decimals)
		local s = slider(p, text, switchHelp(name), lo, hi, decimals, function(v)
			switch(name, decimals == 0 and math.Round(v) or string.format("%." .. decimals .. "f", v), true)
		end)
		bindSwitch(name, function(v) s:SetValue(tonumber(v) or 0) end)
	end
	num("max_agents", "Homers with NM at most", 0, 32, 0)
	num("threads", "Threads (0: auto)", 0, 16, 0)
	num("lod_distance", "LOD distance (units)", 0, 10000, 0)
	num("lod_still", "Still for (s)", 0, 30, 1)
	num("autoattach_max", "Death ragdolls with NM at most", 0, 16, 0)
	num("autoattach_time", "A death ragdoll keeps NM for (s)", 0, 120, 0)
end

-- The server's console commands for looking into DMS: they print to your console (~).
local COMMANDS = {
	{ "Status", "dms_status", "The agents: their flags, the gravity compensation, NM's cost a step, threads and LOD." },
	{ "Self-test", "dms_selftest", "Homer in NM's own physics on a flat floor for 2 s under bodyBalance: he has to stay up. Checks NM's data, not the fork." },
	{ "NM's data", "dms_data", "Loads NM's data from your GTA IV (once a game) and says what was read, or what's missing." },
	{ "Physics", "dms_info", "The module, its IPhysicsNM and the physics environment." },
	{ "Tuning", "dms_tuning_status", "The tuning profile in use and its values." },
	{ "Behaviours", "dms_nm_messages", "Every NM message (behaviour); dms_nm_messages <name> lists its params." },
	{ "Parts", "dms_nm_parts", "NM's 21 parts of Homer, in order (the part numbers reactions take)." },
	{ "Joints", "dms_nm_joints", "NM's joints of Homer: pivots, frames, limits, armature." },
}

local function debugTab(p)
	heading(p, "Versions")
	note(p, "The module, the Lua addon and the fork come in one release and have to match; homer.mdl is made by its dms-homer.")
	ui.about = note(p, "(the server hasn't answered yet)", C.text)
	ui.problems = note(p, "", C.bad)

	heading(p, "Console commands")
	note(p, "They print to your console: open it with ~ (or the button below).")
	local grid = p:Add("DIconLayout")
	grid:Dock(TOP)
	grid:SetSpaceX(6)
	grid:SetSpaceY(6)
	for _, c in ipairs(COMMANDS) do
		button(grid, c[1], c[3] .. "\n\n" .. c[2], function() RunConsoleCommand(c[2]) end):SetSize(150, 26)
	end
	button(grid, "Open the console", "Shows the game's console, where the commands print.\n\nshowconsole", function() RunConsoleCommand("showconsole") end):SetSize(150, 26)

	heading(p, "Send NM a message")
	local r = row(p)
	local line
	-- The button docks first: FILL takes what's left.
	local go = button(r, "Send", nil, function()
		-- Through the menu's channel: dms_send by RunConsoleCommand would get the line in quotes.
		if line:GetValue() ~= "" then send(line:GetValue()) end
	end)
	go:Dock(RIGHT)
	go:SetWide(80)
	go:DockMargin(6, 0, 0, 0)
	line = r:Add("DTextEntry")
	line:Dock(FILL)
	line:SetPlaceholderText("bodyBalance start=true")
	line.OnEnter = go.DoClick
	help(r, "Send NM a message", "A line as the Playroom's console takes it, to the Homer you look at: \"bodyRelax start=true\", \"applyImpulse start=true partIndex=9 impulse=0,30,0\". Points are in NM's frame (metres).\n\ndms_send <line>")

	heading(p, "Logs")
	local log = check(p, "Print the Playroom's reaction log", switchHelp("react_log"), function(v) switch("react_log", v and 1 or 0) end)
	bindSwitch("react_log", function(v) log:SetChecked(tonumber(v) ~= 0) end)
	check(p, "Debug overlay", "NM's capsules, drives and centre of mass over Homer (superadmins). The DMS tool's Inspector shows one Homer's state.\n\ndms_overlay"):SetConVar("dms_overlay")
end

-- The Debug tab's versions and problems, from the catalogue.
local function fillAbout()
	local a = catalogue and catalogue.about
	if not a or not IsValid(ui.about) then return end
	ui.about:SetText(string.format("DMS module %s, Lua addon %s, Delirium %s\nIPhysicsNM: %s\nhomer.mdl: model %s (this release: %s)\nGTA IV: %s\nNM's data: %s",
		tostring(a.module), tostring(a.addon), tostring(a.delirium), tostring(a.interface), tostring(a.modelMade),
		tostring(a.model), a.gameDir ~= "" and a.gameDir or "(none: dms_game_dir)", tostring(a.data)))
	local ps = a.problems or {}
	ui.problems:SetText(#ps == 0 and "" or ("Problems:\n- " .. table.concat(ps, "\n- ") .. "\nSee docs/install.md (FAQ)."))
end

-- One behaviour's params: a slider, a checkbox or a text box each, its doc in the help pane and the tooltip.
local function fillParams()
	local list, msg = ui.params, ui.msg
	if not IsValid(list) then return end
	list:Clear()
	if not msg then return ui.paramsHead:SetText("Pick a behaviour on the left") end
	local ps = paramsOf[msg]
	if not ps then
		-- A behaviour's params come from the server the first time it's picked.
		ui.paramsHead:SetText(msg .. ": loading...")
		if not asked[msg] then
			asked[msg] = true
			request({ kind = "params", name = msg })
		end
		return
	end
	local unused = 0
	for _, p in ipairs(ps) do if not p.used then unused = unused + 1 end end
	ui.paramsHead:SetText(string.format("%s: %d params%s", msg, #ps, unused > 0 and string.format(", %d the port doesn't use (*)", unused) or ""))
	quietly(function()
		for _, p in ipairs(ps) do
			local r = list:Add("DPanel")
			r:Dock(TOP)
			r:SetTall(26)
			r:DockMargin(0, 1, 0, 1)
			r.param = p
			-- A mark on the edited ones.
			r.Paint = function(_, _, h)
				if value(msg, p) == nil then return end
				surface.SetDrawColor(C.accent)
				surface.DrawRect(0, 3, 2, h - 6)
			end
			help(r, p.name, (p.doc ~= "" and p.doc or "(no description)")
				.. "\n\nKind: " .. p.kind .. "; the Playroom's: " .. vecText(p.default)
				.. (p.used and "" or "\n\n* The port takes it but doesn't use it: changing it does nothing."))
			r:SetTooltip(p.doc ~= "" and p.doc or p.name)
			local text = (p.used and "" or "* ") .. p.name
			local color = p.used and C.text or C.dim
			local cur = value(msg, p)
			local reset = r:Add("DImageButton")
			reset:Dock(RIGHT)
			reset:DockMargin(4, 5, 0, 5)
			reset:SetWide(16)
			reset:SetImage("icon16/arrow_undo.png")
			reset:SetTooltip("Back to the Playroom's value")
			reset.DoClick = function() setValue(msg, p.name, nil) fillParams() end
			if p.kind == "bool" then
				local c = r:Add("DCheckBoxLabel")
				c:Dock(FILL)
				c:DockMargin(6, 5, 0, 0)
				c:SetFont("DMS_Text")
				c:SetTextColor(color)
				c:SetText(text)
				c:SetChecked(cur ~= nil and (cur == "true" or cur == "1") or (tonumber(p.default) or 0) ~= 0)
				c.OnChange = function(_, v) if filling == 0 then setValue(msg, p.name, v and "true" or "false") end end
				r.input = c
			elseif p.kind == "float" or p.kind == "int" then
				local lo, hi = range(p)
				local s = r:Add("DNumSlider")
				s:Dock(FILL)
				s:DockMargin(6, 0, 0, 0)
				s:SetText(text)
				s.Label:SetFont("DMS_Text")
				s.Label:SetTextColor(color)
				s.TextArea:SetTextColor(C.text)
				s:SetMinMax(lo, hi)
				s:SetDecimals(p.kind == "int" and 0 or 3)
				s:SetValue(tonumber(cur) or tonumber(p.default) or 0)
				s.OnValueChanged = function(_, v)
					if filling > 0 then return end
					setValue(msg, p.name, p.kind == "int" and tostring(math.Round(v)) or string.format("%.4g", v))
				end
				r.input = s
			else
				label(r, text, 200):SetTextColor(color)
				local t = r:Add("DTextEntry")
				t:Dock(FILL)
				t:DockMargin(0, 2, 0, 2)
				t:SetValue(cur or vecText(p.default))
				t.OnEnter = function(self)
					local v = self:GetValue():gsub("%s", "")
					setValue(msg, p.name, v ~= "" and v or nil)
				end
				t.OnLoseFocus = t.OnEnter
				r.input = t
			end
		end
	end)
end

local function pick(msg)
	ui.msg, lastMsg = msg, msg
	fillParams()
end

-- NM's behaviours, those matching the search. Which one is picked is drawn, not selected, so refilling the list
-- never picks anything.
local function fillBehaviours()
	local list = ui.behaviours
	if not IsValid(list) or not catalogue then return end
	list:Clear()
	local find = ui.search:GetValue():lower()
	for _, name in ipairs(catalogue.messages or {}) do
		if find == "" or name:lower():find(find, 1, true) then
			local b = list:Add("DButton")
			b:Dock(TOP)
			b:SetTall(22)
			b:DockMargin(0, 0, 0, 1)
			b:SetFont("DMS_Text")
			b:SetContentAlignment(4)
			b:SetTextInset(10, 0)
			b:SetText(name)
			b:SetTextColor(C.text)
			b.Paint = function(self, w, h)
				local picked = ui.msg == name
				self:SetTextColor(picked and C.accent or C.text)
				if picked or self:IsHovered() then
					surface.SetDrawColor(C.hover)
					surface.DrawRect(0, 0, w, h)
				end
				if editing.profile.messages[name] then
					surface.SetDrawColor(C.accent)
					surface.DrawRect(2, 6, 3, h - 12)
				end
			end
			help(b, name, "NM's behaviour (or message): its params on the right. A mark on the left: the profile changes some of them.")
			b.DoClick = function() pick(name) end
		end
	end
end

local function tuningTab(p)
	local top = p:Add("DPanel")
	top:Dock(TOP)
	top:SetTall(100)
	top.Paint = nil
	heading(top, "Tuning (docs/adr/0009)")
	ui.on = check(top, "Tuning on", "Only through NM's own inputs: the profile's values are added to the messages that start a behaviour or change its params, and its taskparams go over the game's. Off: the Playroom's, as golden runs always are.\n\ndms_tuning",
		function(v) request({ kind = "use", name = editing.name, on = v }) end)

	local r = row(top)
	local prof = r:Add("DComboBox")
	prof:Dock(LEFT)
	prof:SetWide(170)
	prof:SetSortItems(false)
	help(prof, "Profile", "The profile in use. A profile holds only what differs from the Playroom: data/dms/profiles/<name>.json on the server.\n\ndms_tuning_profile")
	prof.OnSelect = function(_, _, name)
		if filling > 0 then return end
		request({ kind = "profile", name = name })
		request({ kind = "use", name = name, on = ui.on:GetChecked() })
	end
	ui.profile = prof
	local nameEntry = r:Add("DTextEntry")
	nameEntry:Dock(LEFT)
	nameEntry:DockMargin(6, 0, 0, 0)
	nameEntry:SetWide(150)
	nameEntry:SetPlaceholderText("save as...")
	help(nameEntry, "Profile name", "Save writes the values edited here under this name (empty: the profile being edited). The Playroom profile can't be changed.")
	local function rowButton(text, helpText, fn)
		local b = button(r, text, helpText, fn)
		b:Dock(LEFT)
		b:DockMargin(6, 0, 0, 0)
		b:SetWide(84)
	end
	rowButton("Save", "Saves the edited values (behaviours' params and taskparams) and makes this the profile in use.", function()
		local name = nameEntry:GetValue() ~= "" and nameEntry:GetValue() or editing.name
		if name == "Playroom" then return notification.AddLegacy("DMS: the Playroom profile can't be changed: give a name", NOTIFY_ERROR, 4) end
		editing.name = name
		request({ kind = "save", name = name, profile = editing.profile })
	end)
	rowButton("Delete", "Deletes the profile being edited from the server.", function()
		if editing.name == "Playroom" then return end
		local name = editing.name
		Derma_Query("Delete the profile " .. name .. "?", "DMS", "Delete", function() request({ kind = "delete", name = name }) end, "Cancel")
	end)
	rowButton("Playroom", "Back to the Playroom: tuning off, the Playroom's profile; what was edited here and not saved is dropped.", function()
		editing = { name = "Playroom", profile = { messages = {}, taskparams = {} } }
		request({ kind = "reset" })
	end)

	local split = p:Add("DPanel")
	split:Dock(FILL)
	split:DockMargin(0, 6, 0, 0)
	split.Paint = nil
	local left = split:Add("DPanel")
	left:Dock(LEFT)
	left:SetWide(230)
	left:DockPadding(4, 4, 4, 4)
	left.Paint = paintPane
	ui.search = left:Add("DTextEntry")
	ui.search:Dock(TOP)
	ui.search:SetPlaceholderText("find a behaviour")
	ui.search.OnChange = fillBehaviours
	ui.behaviours = styleScroll(left:Add("DScrollPanel"))
	ui.behaviours:Dock(FILL)
	ui.behaviours:DockMargin(0, 4, 0, 0)

	local right = split:Add("DPanel")
	right:Dock(FILL)
	right:DockMargin(6, 0, 0, 0)
	right:DockPadding(8, 4, 4, 4)
	right.Paint = paintPane
	ui.paramsHead = right:Add("DLabel")
	ui.paramsHead:Dock(TOP)
	ui.paramsHead:SetTall(24)
	ui.paramsHead:SetFont("DMS_Head")
	ui.paramsHead:SetTextColor(C.text)
	local try = button(right, "Start it on Homer (with these values)", "Sends `<behaviour> start=true` and the edited values to the Homer you look at.", function()
		if not ui.msg then return end
		local kv = { ui.msg, "start=true" }
		for k, v in SortedPairs(editing.profile.messages[ui.msg] or {}) do kv[#kv + 1] = k .. "=" .. v end
		send(table.concat(kv, " "))
	end)
	try:Dock(BOTTOM)
	try:DockMargin(0, 4, 0, 0)
	ui.params = styleScroll(right:Add("DScrollPanel"))
	ui.params:Dock(FILL)
	ui.params:GetCanvas():DockPadding(0, 0, 8, 0)
end

-- The game's taskparams sets (the tasks' tuning), editable value by value.
local function fillTaskParams()
	local list = ui.tp
	if not IsValid(list) or not catalogue then return end
	list:Clear()
	heading(list, "The game's taskparams")
	local tp = catalogue.taskparams or {}
	if next(tp) == nil then
		note(list, "This game has no taskparams.txt (the PC version): NM's tasks run their defaults, nothing to tune here.", C.text)
		return
	end
	note(list, "The tasks' sets (shot, melee, explosion, fire, high fall, ...); saved with the profile on the Tuning tab.")
	quietly(function()
		for task, sets in SortedPairs(tp) do
			local cat = list:Add("DCollapsibleCategory")
			cat:Dock(TOP)
			cat:DockMargin(0, 2, 0, 2)
			cat:SetLabel(task)
			cat:SetExpanded(false)
			local inner = vgui.Create("DListLayout")
			inner:DockPadding(6, 2, 6, 4)
			cat:SetContents(inner)
			for set, params in SortedPairs(sets) do
				for p, v in SortedPairs(params) do
					local key = task .. " " .. set .. " " .. p
					local s = vgui.Create("DNumSlider")
					s:SetText(set .. " " .. p)
					s.Label:SetTextColor(C.text)
					s.TextArea:SetTextColor(C.text)
					local m = math.max(math.abs(v) * 4, 1)
					s:SetMinMax(math.min(v - m, 0), v + m)
					s:SetDecimals(3)
					s:SetValue(editing.profile.taskparams[key] or v)
					s.OnValueChanged = function(_, x)
						if filling > 0 then return end
						editing.profile.taskparams[key] = math.abs(x - v) > 1e-6 and x or nil
					end
					help(s, key, "The game's value: " .. v)
					inner:Add(s)
				end
			end
		end
	end)
end

local function refresh()
	if not catalogue or not IsValid(ui.frame) then return end
	fillBehaviours()
	quietly(function()
		ui.on:SetChecked(catalogue.on and true or false)
		ui.profile:Clear()
		for _, n in ipairs(catalogue.profiles or {}) do ui.profile:AddChoice(n, nil, n == catalogue.profile) end
	end)
	applySwitches(catalogue.switches)
	fillTaskParams()
	fillParams()
	fillAbout()
end

-- The window -----------------------------------------------------------------------------------------------------

local TABS = {
	{ "Homer", "icon16/user.png", homerTab },
	{ "Reactions", "icon16/lightning.png", reactionsTab },
	{ "Tuning", "icon16/wrench.png", tuningTab, true },
	{ "Task params", "icon16/table.png", function(p) ui.tp = p end },
	{ "Performance", "icon16/server.png", performanceTab },
	{ "Debug", "icon16/bug.png", debugTab },
}

local DEFAULT_HELP = { "DMS", "Point at anything to see what it does.\n\nThe window: dms_menu, or a key from the Homer tab." }

local function open()
	if IsValid(ui.frame) then ui.frame:Remove() end
	ui = { switch = {}, msg = lastMsg }
	local f = vgui.Create("DFrame")
	ui.frame = f
	f:SetSize(math.min(ScrW() - 40, 1060), math.min(ScrH() - 40, 680))
	f:Center()
	f:SetTitle("")
	f:SetSizable(true)
	f:SetMinWidth(820)
	f:SetMinHeight(480)
	f.btnMaxim:SetVisible(false)
	f.btnMinim:SetVisible(false)
	f.btnClose.Paint = function(self, w, h)
		local c = self:IsHovered() and C.bad or C.dim
		surface.SetDrawColor(c)
		surface.DrawLine(w / 2 - 5, h / 2 - 5, w / 2 + 5, h / 2 + 5)
		surface.DrawLine(w / 2 + 5, h / 2 - 5, w / 2 - 5, h / 2 + 5)
	end
	f:DockPadding(10, 40, 10, 10)
	f:MakePopup()
	local opened = SysTime()
	local admin = LocalPlayer():IsSuperAdmin()
	f.Paint = function(_, w, h)
		Derma_DrawBackgroundBlur(f, opened)
		surface.SetDrawColor(C.bg)
		surface.DrawRect(0, 0, w, h)
		surface.SetDrawColor(C.line)
		surface.DrawOutlinedRect(0, 0, w, h)
		draw.SimpleText("DMS", "DMS_Title", 12, 8, C.accent)
		draw.SimpleText(admin and "Delirium on Homer" or "Delirium on Homer  -  not a superadmin: the server's settings won't change",
			"DMS_Text", 64, 13, admin and C.dim or C.bad)
	end

	local side = f:Add("DPanel")
	side:Dock(LEFT)
	side:SetWide(150)
	side.Paint = nil
	local helpPane = f:Add("DPanel")
	helpPane:Dock(RIGHT)
	helpPane:SetWide(250)
	helpPane:DockMargin(8, 0, 0, 0)
	helpPane:DockPadding(10, 8, 10, 8)
	helpPane.Paint = paintPane
	local helpTitle = helpPane:Add("DLabel")
	helpTitle:Dock(TOP)
	helpTitle:SetFont("DMS_Head")
	helpTitle:SetTextColor(C.accent)
	helpTitle:SetWrap(true)
	helpTitle:SetAutoStretchVertical(true)
	local helpText = helpPane:Add("DLabel")
	helpText:Dock(TOP)
	helpText:DockMargin(0, 6, 0, 0)
	helpText:SetFont("DMS_Text")
	helpText:SetTextColor(C.text)
	helpText:SetWrap(true)
	helpText:SetAutoStretchVertical(true)
	local content = f:Add("DPanel")
	content:Dock(FILL)
	content:DockMargin(8, 0, 0, 0)
	content.Paint = nil

	local pages, buttons = {}, {}
	local function show(name)
		lastTab = name
		for n, pg in pairs(pages) do pg:SetVisible(n == name) end
		for n, b in pairs(buttons) do b.dmsActive = n == name end
	end
	for _, t in ipairs(TABS) do
		local name = t[1]
		local pg = content:Add("DPanel")
		pg:Dock(FILL)
		pg.Paint = nil
		-- The tuning tab lays itself out; the others scroll.
		t[3](t[4] and pg or page(pg))
		pages[name] = pg
		local b = button(side, name, nil, function() show(name) end)
		b:Dock(TOP)
		b:DockMargin(0, 0, 0, 4)
		b:SetTall(32)
		b:SetContentAlignment(4)
		b:SetTextInset(32, 0)
		local icon = b:Add("DImage")
		icon:SetPos(9, 8)
		icon:SetSize(16, 16)
		icon:SetImage(t[2])
		icon:SetMouseInputEnabled(false)
		buttons[name] = b
	end
	show(pages[lastTab] and lastTab or "Homer")

	-- The help pane follows the cursor (no tooltip's wait). Nothing here talks to the server.
	local base, shown = f.Think, nil
	f.Think = function(self)
		base(self)
		local h = vgui.GetHoveredPanel()
		while IsValid(h) and not h.dmsHelp and h ~= self do h = h:GetParent() end
		local d = IsValid(h) and h.dmsHelp or DEFAULT_HELP
		if d ~= shown then
			shown = d
			helpTitle:SetText(d[1])
			helpText:SetText(isfunction(d[2]) and d[2]() or d[2])
		end
	end
	ui.pick, ui.show = pick, show
	f.dms = ui
	refresh()
	request({ kind = "catalogue" })
	return f
end

local function toggle()
	if IsValid(ui.frame) then ui.frame:Close() else open() end
end

concommand.Add("dms_menu", toggle, nil, "Opens and closes the DMS window")
DMS_MenuOpen = open

-- The catalogue for the DMS tool (its behaviours and weapons): what came last, and a request for it (the hook
-- DMS_Catalogue runs when it comes; superadmins only).
function DMS_MenuCatalogue() return catalogue end
function DMS_MenuAskCatalogue() request({ kind = "catalogue" }) end

-- dms_menu_key, pressed outside text boxes, the chat and the console, opens or closes the window.
local keyWasDown = false
hook.Add("Think", "DMS_MenuKey", function()
	local k = menuKey:GetInt()
	if k <= 0 then return end
	local down = input.IsKeyDown(k)
	if down and not keyWasDown and IsValid(LocalPlayer()) then
		local focus = vgui.GetKeyboardFocus()
		local typing = IsValid(focus) and focus:GetClassName() == "TextEntry" or LocalPlayer():IsTyping()
		if not typing and not gui.IsGameUIVisible() and not gui.IsConsoleVisible() then toggle() end
	end
	keyWasDown = down
end)

-- Replies come in pieces (tuning.lua): put together by their id.
local pieces = {}
net.Receive("dms_tuning", function()
	local id, i, n = net.ReadUInt(16), net.ReadUInt(16), net.ReadUInt(16)
	local len = net.ReadUInt(32)
	local p = pieces[id] or {}
	pieces[id] = p
	p[i] = net.ReadData(len)
	if #p < n then return end
	pieces[id] = nil
	local r = util.JSONToTable(util.Decompress(table.concat(p)) or "")
	if not r then return end
	if r.kind == "params" then
		paramsOf[r.data.name] = r.data.params or {}
		asked[r.data.name] = nil
		if ui.msg == r.data.name then fillParams() end
	elseif r.kind == "catalogue" then
		catalogue = r.data
		if catalogue.error then notification.AddLegacy("DMS: " .. catalogue.error, NOTIFY_ERROR, 6) end
		if editing.name ~= catalogue.profile then request({ kind = "profile", name = catalogue.profile }) end
		refresh()
		hook.Run("DMS_Catalogue", catalogue)
	elseif r.kind == "switches" then
		if catalogue then catalogue.switches = r.data end
		applySwitches(r.data)
	elseif r.kind == "profile" then
		editing = { name = r.data.name, profile = r.data.profile }
		editing.profile.messages = editing.profile.messages or {}
		editing.profile.taskparams = editing.profile.taskparams or {}
		refresh()
	elseif r.kind == "error" then
		notification.AddLegacy("DMS: " .. tostring(r.data), NOTIFY_ERROR, 4)
	end
end)

-- The context menu's icon (hold C) and the spawn menu's entry open the window too.
list.Set("DesktopWindows", "DMS", {
	title = "DMS",
	icon = "icon64/playermodel.png",
	init = function(_, window)
		window:Remove()
		open()
	end,
})

hook.Add("PopulateToolMenu", "DMS_Menu", function()
	spawnmenu.AddToolMenuOption("Utilities", "DMS", "DMS_Homer", "Homer (Delirium)", "", "", function(panel)
		panel:ClearControls()
		panel:Help("DMS has its own window: Homer, the Playroom's reactions, tuning, the agents' limits.")
		panel:Button("Open the DMS window", "dms_menu")
		panel:Help("Console: dms_menu. A key for it: the window's Homer tab.")
	end)
end)

hook.Add("AddToolMenuCategories", "DMS_Menu", function()
	spawnmenu.AddToolCategory("Utilities", "DMS", "DMS")
end)
