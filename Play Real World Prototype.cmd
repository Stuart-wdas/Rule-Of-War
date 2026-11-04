@echo off
rem This prototype keeps its own executable. Pass --rebuild to refresh it.
cd /d "%~dp0"
set "ROW_GIS_BINARY=%~dp0prototypes\real-world-hottot\bin\row-gis-prototype.exe"
if exist "%ROW_GIS_BINARY%" if not "%~1"=="--rebuild" goto launch
if not defined CARGO_TARGET_DIR set "CARGO_TARGET_DIR=%~dp0native\target"
if not defined CARGO_INCREMENTAL set "CARGO_INCREMENTAL=0"
cargo build --locked --no-default-features
if errorlevel 1 (
  pause
  exit /b 1
)
if not exist "%~dp0prototypes\real-world-hottot\bin" mkdir "%~dp0prototypes\real-world-hottot\bin"
copy /y "%CARGO_TARGET_DIR%\debug\rule-of-war.exe" "%ROW_GIS_BINARY%" >nul
if errorlevel 1 exit /b 1
:launch
if not exist "%LOCALAPPDATA%\RuleOfWar\runtime" mkdir "%LOCALAPPDATA%\RuleOfWar\runtime"
set "ROW_GIS_RUNTIME=%LOCALAPPDATA%\RuleOfWar\runtime\row-gis-%RANDOM%-%RANDOM%.exe"
copy /y "%ROW_GIS_BINARY%" "%ROW_GIS_RUNTIME%" >nul
if errorlevel 1 exit /b 1
"%ROW_GIS_RUNTIME%" --gis-prototype %*
set "ROW_GIS_EXIT=%ERRORLEVEL%"
del /q "%ROW_GIS_RUNTIME%"
exit /b %ROW_GIS_EXIT%
