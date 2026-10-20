@echo off
chcp 65001 <nul >nul
setlocal EnableExtensions EnableDelayedExpansion

if not "%~1"=="" (
    echo Usage: install-release.bat
    exit /b 2
)

for %%I in ("%~dp0.") do set "project_directory=%%~fI"
set "source_work=%project_directory%\skills\work"
set "built_work=%project_directory%\bin\work.exe"
set "missing_source="
set "work_install_mode=release"

for %%F in (verify-package.ps1 global-agents.bat work-core.bat) do (
    if not exist "%~dp0installer\%%F" (
        echo Error: release installer module is missing: %%F.
        exit /b 1
    )
)
powershell.exe -NoProfile -File "%~dp0installer\verify-package.ps1" -PackageRoot "%project_directory%"
if errorlevel 1 exit /b 1
if /I not "%PROCESSOR_ARCHITECTURE%"=="AMD64" (
    echo Error: this release package requires Windows x64.
    exit /b 1
)
findstr /c:"x86_64-pc-windows-msvc" "%project_directory%\release-info.json" >nul
if errorlevel 1 (
    echo Error: release package target is not Windows x64 MSVC.
    exit /b 1
)
if not exist "%built_work%" (
    echo Error: packaged Work binary is missing.
    exit /b 1
)
"%built_work%" --help >nul 2>nul
if errorlevel 1 (
    echo Error: packaged Work binary failed its startup check.
    exit /b 1
)

call "%~dp0installer\global-agents.bat" "%project_directory%\AGENTS.md"
if errorlevel 1 exit /b 1
call "%~dp0installer\work-core.bat"
exit /b %errorlevel%
