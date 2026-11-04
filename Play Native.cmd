@echo off
cd /d "%~dp0"
if not defined CARGO_TARGET_DIR set "CARGO_TARGET_DIR=%~dp0native\target"
rem Reuse the validated build and avoid retaining another large incremental cache.
if not defined CARGO_INCREMENTAL set "CARGO_INCREMENTAL=0"
set "ROW_BUILD_FLAGS="
if not exist ".data\dlss-tools\DLSS\include\nvsdk_ngx.h" set "ROW_BUILD_FLAGS=--no-default-features"
if not exist ".data\dlss-tools\Vulkan-Headers\include\vulkan\vulkan.h" set "ROW_BUILD_FLAGS=--no-default-features"
if not exist ".data\dlss-tools\clang-runtime\clang\native\libclang.dll" set "ROW_BUILD_FLAGS=--no-default-features"
where cargo >nul 2>nul
if not errorlevel 1 (
  if defined ROW_BUILD_FLAGS (
    if exist "%CARGO_TARGET_DIR%\release\rule-of-war.exe" goto prebuilt
    if exist "%CARGO_TARGET_DIR%\debug\rule-of-war.exe" goto prebuilt
  )
  cargo build --locked %ROW_BUILD_FLAGS%
  if errorlevel 1 (
    pause
    exit /b 1
  )
  set "GAME_SOURCE=%CARGO_TARGET_DIR%\debug\rule-of-war.exe"
  goto launch
)
:prebuilt
if exist "%CARGO_TARGET_DIR%\release\rule-of-war.exe" (
  set "GAME_SOURCE=%CARGO_TARGET_DIR%\release\rule-of-war.exe"
  goto launch
)
if exist "%CARGO_TARGET_DIR%\debug\rule-of-war.exe" (
  set "GAME_SOURCE=%CARGO_TARGET_DIR%\debug\rule-of-war.exe"
  goto launch
)
if exist "native\target\release\rule-of-war.exe" (
  set "GAME_SOURCE=%~dp0native\target\release\rule-of-war.exe"
  goto launch
)
if exist "native\target\debug\rule-of-war.exe" (
  set "GAME_SOURCE=%~dp0native\target\debug\rule-of-war.exe"
  goto launch
)
if exist "%LOCALAPPDATA%\RuleOfWar\build-cache\debug\rule-of-war.exe" (
  set "GAME_SOURCE=%LOCALAPPDATA%\RuleOfWar\build-cache\debug\rule-of-war.exe"
  goto launch
)
echo Rust is required to build this game. Install it from https://rustup.rs
pause
exit /b 1

:launch
if not exist "%LOCALAPPDATA%\RuleOfWar\runtime" mkdir "%LOCALAPPDATA%\RuleOfWar\runtime"
set "GAME_RUNTIME=%LOCALAPPDATA%\RuleOfWar\runtime\rule-of-war-%RANDOM%-%RANDOM%.exe"
copy /y "%GAME_SOURCE%" "%GAME_RUNTIME%" >nul
if errorlevel 1 (
  echo The game could not be copied to its runtime folder.
  pause
  exit /b 1
)
for %%F in ("%GAME_SOURCE%") do set "GAME_BIN_DIR=%%~dpF"
if exist "%GAME_BIN_DIR%nvngx_dlss.dll" copy /y "%GAME_BIN_DIR%nvngx_dlss.dll" "%LOCALAPPDATA%\RuleOfWar\runtime\nvngx_dlss.dll" >nul
if exist "%GAME_BIN_DIR%NVIDIA-DLSS-LICENSE.txt" copy /y "%GAME_BIN_DIR%NVIDIA-DLSS-LICENSE.txt" "%LOCALAPPDATA%\RuleOfWar\runtime\NVIDIA-DLSS-LICENSE.txt" >nul
if exist "%GAME_BIN_DIR%NVIDIA-DLSS-NOTICES.txt" copy /y "%GAME_BIN_DIR%NVIDIA-DLSS-NOTICES.txt" "%LOCALAPPDATA%\RuleOfWar\runtime\NVIDIA-DLSS-NOTICES.txt" >nul
"%GAME_RUNTIME%" %*
set "GAME_EXIT=%ERRORLEVEL%"
del /q "%GAME_RUNTIME%"
exit /b %GAME_EXIT%
