if not util.IsBinaryModuleInstalled("dms") then
	ErrorNoHalt("[DMS] gmsv_dms isn't installed: put gmsv_dms_win64.dll into garrysmod/lua/bin (x86-64 branch only)\n")
	return
end

require("dms")
