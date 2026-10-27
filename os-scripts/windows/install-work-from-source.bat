@echo off
chcp 65001 <nul >nul
setlocal DisableDelayedExpansion
for %%I in ("%~dp0..\..") do set "project_directory=%%~fI"
powershell.exe -NoProfile -File "%~dp0installer\install\install-core.ps1" -Mode source -Root "%project_directory%" %*
exit /b %errorlevel%
