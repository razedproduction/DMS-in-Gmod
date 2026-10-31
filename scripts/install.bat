@echo off
setlocal EnableExtensions DisableDelayedExpansion
title DMS Installer

cd /d "%~dp0"
set "HERE=%~dp0"

echo ==========================================
echo          DMS Installer for Garry's Mod
echo ==========================================
echo.
echo This script installs DMS, creates Homer's model,
echo or restores the original Steam vphysics.dll.
echo.

:ask_action
echo Choose an action:
echo   1. Install DMS
echo   2. Install DMS and create Homer's model
echo   3. Create / recreate only Homer's model
echo   4. Uninstall DMS
echo.
set "ACTION="
set /p "ACTION=Enter 1, 2, 3 or 4: "
if "%ACTION%"=="1" goto action_install
if "%ACTION%"=="2" goto action_install_model
if "%ACTION%"=="3" goto action_model
if "%ACTION%"=="4" goto action_uninstall
echo Invalid choice. Try again.
echo.
goto ask_action

:action_install
set "DO_INSTALL=1"
set "DO_MODEL=0"
goto get_gmod

:action_install_model
set "DO_INSTALL=1"
set "DO_MODEL=1"
goto get_gmod

:action_model
set "DO_INSTALL=0"
set "DO_MODEL=1"
goto get_gmod

:action_uninstall
set "DO_INSTALL=0"
set "DO_MODEL=0"
set "DO_UNINSTALL=1"
goto get_gmod

:get_gmod
echo.
set "GMOD="
for /f "tokens=2,*" %%A in ('reg query "HKCU\Software\Valve\Steam" /v SteamPath 2^>nul ^| find /i "SteamPath"') do set "STEAM=%%B"
if defined STEAM if exist "%STEAM%\steamapps\common\GarrysMod\garrysmod" set "GMOD=%STEAM%\steamapps\common\GarrysMod"

if defined GMOD (
    echo Detected Garry's Mod:
    echo "%GMOD%"
    set "USE_DETECTED="
    set /p "USE_DETECTED=Use this folder? [Y/n]: "
    if /i "%USE_DETECTED%"=="n" set "GMOD="
)

if not defined GMOD (
    echo Enter the full path to your Garry's Mod folder.
    echo Example: D:\SteamLibrary\steamapps\common\GarrysMod
    set /p "GMOD=Garry's Mod folder: "
)

if not defined GMOD (
    echo ERROR: No Garry's Mod folder was entered.
    goto fail
)
if not exist "%GMOD%\garrysmod\" (
    echo ERROR: This folder does not contain the "garrysmod" subfolder:
    echo "%GMOD%"
    goto fail
)

set "WIN64=%GMOD%\bin\win64"
set "GAMEDATA=%GMOD%\garrysmod"
set "ORIGINAL=%WIN64%\vphysics.dll.original"

if not exist "%WIN64%\vphysics.dll" (
    echo ERROR: "%WIN64%\vphysics.dll" was not found.
    echo DMS requires the Garry's Mod x86-64 branch.
    echo In Steam: Garry's Mod - Properties - Betas - x86-64.
    goto fail
)

tasklist /FI "IMAGENAME eq hl2.exe" 2>nul | find /i "hl2.exe" >nul
if not errorlevel 1 (
    echo ERROR: Garry's Mod appears to be running. Close it first.
    goto fail
)
tasklist /FI "IMAGENAME eq gmod.exe" 2>nul | find /i "gmod.exe" >nul
if not errorlevel 1 (
    echo ERROR: Garry's Mod appears to be running. Close it first.
    goto fail
)

if "%DO_UNINSTALL%"=="1" goto uninstall

if "%DO_INSTALL%"=="1" (
    if not exist "%HERE%bin\vphysics.dll" (
        echo ERROR: Missing "%HERE%bin\vphysics.dll"
        goto fail
    )
    if not exist "%HERE%bin\gmsv_dms_win64.dll" (
        echo ERROR: Missing "%HERE%bin\gmsv_dms_win64.dll"
        goto fail
    )
    if not exist "%HERE%addon\" (
        echo ERROR: Missing "%HERE%addon" folder
        goto fail
    )

    if not exist "%ORIGINAL%" (
        copy /y "%WIN64%\vphysics.dll" "%ORIGINAL%" >nul
        if errorlevel 1 (
            echo ERROR: Could not back up the original vphysics.dll.
            goto fail
        )
    )

    copy /y "%HERE%bin\vphysics.dll" "%WIN64%\vphysics.dll" >nul
    if errorlevel 1 (
        echo ERROR: Could not copy vphysics.dll. Try running as administrator.
        goto fail
    )
    echo Installed: "%WIN64%\vphysics.dll"

    if not exist "%GAMEDATA%\lua\bin" mkdir "%GAMEDATA%\lua\bin"
    copy /y "%HERE%bin\gmsv_dms_win64.dll" "%GAMEDATA%\lua\bin\gmsv_dms_win64.dll" >nul
    if errorlevel 1 (
        echo ERROR: Could not copy gmsv_dms_win64.dll.
        goto fail
    )
    echo Installed: "%GAMEDATA%\lua\bin\gmsv_dms_win64.dll"

    if exist "%GAMEDATA%\addons\dms" rmdir /s /q "%GAMEDATA%\addons\dms"
    xcopy "%HERE%addon" "%GAMEDATA%\addons\dms\" /e /i /h /y >nul
    if errorlevel 1 (
        echo ERROR: Could not copy the DMS addon.
        goto fail
    )
    echo Installed: "%GAMEDATA%\addons\dms"
)

if "%DO_MODEL%"=="1" goto get_gta

if not exist "%GAMEDATA%\models\dms\homer.mdl" (
    echo.
    echo Homer's model has not been created yet.
    echo Run this script again and choose option 2 or 3 to create it.
)
goto done

:get_gta
echo.
echo Enter the full path to your GTA IV folder.
echo PC example: D:\Games\Grand Theft Auto IV
echo Xbox 360 dump: the folder containing default.xex
set "GTA="
set /p "GTA=GTA IV folder: "

if not defined GTA (
    echo ERROR: No GTA IV folder was entered.
    goto fail
)
if not exist "%GTA%\GTAIV.exe" if not exist "%GTA%\default.xex" (
    echo ERROR: This does not look like a GTA IV folder.
    echo Expected GTAIV.exe or default.xex in the selected folder.
    goto fail
)
if not exist "%HERE%bin\dms-homer.exe" (
    echo ERROR: Missing "%HERE%bin\dms-homer.exe"
    goto fail
)

if not exist "%GAMEDATA%\data\dms" mkdir "%GAMEDATA%\data\dms"
> "%GAMEDATA%\data\dms\game_dir.txt" echo %GTA%
echo Saved GTA IV folder to:
echo "%GAMEDATA%\data\dms\game_dir.txt"
echo.
echo Creating Homer's model...
"%HERE%bin\dms-homer.exe" --game "%GTA%" --gmod "%GMOD%"
if errorlevel 1 (
    echo ERROR: dms-homer.exe failed.
    goto fail
)
goto done

:uninstall
echo.
echo This will restore Steam's vphysics.dll and remove the DMS Lua binary
echo and addon. Your generated models, materials and data will be kept.
set "CONFIRM="
set /p "CONFIRM=Continue with uninstall? [y/N]: "
if /i not "%CONFIRM%"=="y" goto cancelled

if exist "%ORIGINAL%" (
    copy /y "%ORIGINAL%" "%WIN64%\vphysics.dll" >nul
    if errorlevel 1 (
        echo ERROR: Could not restore the original vphysics.dll.
        goto fail
    )
    del /q "%ORIGINAL%"
    echo Restored Steam's vphysics.dll.
) else (
    echo No backup found. The current vphysics.dll was left unchanged.
    echo Steam's "Verify integrity of game files" can restore its file.
)

if exist "%GAMEDATA%\lua\bin\gmsv_dms_win64.dll" del /q "%GAMEDATA%\lua\bin\gmsv_dms_win64.dll"
if exist "%GAMEDATA%\addons\dms" rmdir /s /q "%GAMEDATA%\addons\dms"
echo Removed DMS Lua binary and addon.
echo Kept your files: garrysmod\models\dms, materials\models\dms and data\dms.
goto done

:cancelled
echo Uninstall cancelled.
goto done

:done
echo.
echo ==========================================
echo Done.
echo In Garry's Mod, use dms_homer in the console
echo to spawn Homer, or dms_menu to open the DMS window.
echo ==========================================
echo.
pause
exit /b 0

:fail
echo.
echo Operation stopped because of an error.
echo.
pause
exit /b 1
