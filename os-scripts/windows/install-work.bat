@echo off
chcp 65001 <nul >nul
setlocal EnableExtensions EnableDelayedExpansion

if not "%~1"=="" (
    echo Error: this installer no longer accepts plan, task, execute, or all arguments.
    exit /b 2
)

for %%I in ("%~dp0..\..") do set "project_directory=%%~fI"
set "source_work=%project_directory%\skills\work"
set "missing_source="

if not exist "%~dp0installer\global-agents.bat" (
    echo Error: Global AGENTS.md installer module is missing.
    exit /b 1
)
call "%~dp0installer\global-agents.bat" "%project_directory%\AGENTS.md"
if errorlevel 1 exit /b 1

if not exist "%~dp0installer\work-core.bat" (
    echo Error: Work installer core is missing.
    exit /b 1
)
call "%~dp0installer\work-core.bat"
exit /b %errorlevel%
