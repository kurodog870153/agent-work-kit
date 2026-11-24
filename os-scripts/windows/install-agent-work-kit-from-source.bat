@echo off
chcp 65001 <nul >nul
setlocal DisableDelayedExpansion
set "installer_entry_non_interactive="
:installer_entry_scan
if "%~1"=="" goto installer_entry_main
if "%~1"=="--non-interactive" set "installer_entry_non_interactive=1"
shift /1
goto installer_entry_scan
:installer_entry_main
for %%I in ("%~dp0..\..") do set "project_directory=%%~fI"
powershell.exe -NoProfile -File "%~dp0installer\install\install-core.ps1" -Mode source -Root "%project_directory%" %*
set "result=%errorlevel%"
:installer_entry_finish
if "%installer_entry_non_interactive%"=="1" exit /b %result%
if "%AGENT_WORK_KIT_INSTALLER_NESTED%"=="1" exit /b %result%
if defined CI exit /b %result%
if defined GITHUB_ACTIONS exit /b %result%
powershell.exe -NoProfile -Command "if (-not [Console]::IsInputRedirected) { [Console]::WriteLine('Installer exited (code %result%).'); [Console]::Write('Press Enter to exit...'); [void][Console]::ReadLine() }"
exit /b %result%
