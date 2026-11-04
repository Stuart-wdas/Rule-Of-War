@echo off
if not exist "%~dp0assets\Realistic_Explosion_VFX\mantaflow\complete.json" (
  echo The new fluid renders have not been published yet.
  pause
  exit /b 1
)
start "Rule of War - Mantaflow Explosion Review" "%~dp0assets\Realistic_Explosion_VFX\mantaflow\preview.html"
