@echo off
set "PERFM_EXE=%~dp0Perfm.exe"
powershell.exe -NoProfile -Command "Start-Process -FilePath $env:PERFM_EXE -Verb RunAs"
