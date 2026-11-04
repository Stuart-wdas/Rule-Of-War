@echo off
cd /d "%~dp0"
set "VFX_BLENDER=%~dp0.data\vfx-tools\blender-4.5.3-windows-x64\blender.exe"
if not exist "assets\Realistic_Explosion_VFX\mantaflow\explosion_gallery.blend" if not exist "assets\Realistic_Explosion_VFX\locked_view_hd\explosion_gallery.blend" (
  echo The HD explosion gallery has not been published yet.
  pause
  exit /b 1
)
if not exist "%VFX_BLENDER%" (
  echo This review launcher needs the tested portable Blender 4.5.3 in .data\vfx-tools.
  pause
  exit /b 1
)
"%VFX_BLENDER%" --python "%~dp0assets\Realistic_Explosion_VFX\tools\open_locked_gallery.py"
