@echo off
setlocal
set CARGO_BUILD_JOBS=2
set CARGO_INCREMENTAL=0
call "%~dp0Play Native.cmd" --ground-demo --ground-baseline --ground-close --graphics high %*
