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
for %%I in ("%~dp0.") do set "package_directory=%%~fI"
powershell.exe -NoProfile -File "%package_directory%\installer\package\verify-integrity.ps1" -PackageRoot "%package_directory%"
if errorlevel 1 (
    set "result=1"
    goto installer_entry_finish
)
if /I not "%PROCESSOR_ARCHITECTURE%"=="AMD64" (
    echo Error: this release package requires Windows x64.
    set "result=1"
    goto installer_entry_finish
)
findstr /c:"x86_64-pc-windows-msvc" "%package_directory%\release-info.json" >nul
if errorlevel 1 (
    echo Error: release package target is not Windows x64 MSVC.
    set "result=1"
    goto installer_entry_finish
)
for %%B in (work skill) do (
    if not exist "%package_directory%\bin\%%B.exe" (
        echo Error: packaged %%B binary is missing.
        set "result=1"
        goto installer_entry_finish
    )
    "%package_directory%\bin\%%B.exe" --help >nul 2>nul
    if errorlevel 1 (
        echo Error: packaged %%B binary failed its startup check.
        set "result=1"
        goto installer_entry_finish
    )
)
powershell.exe -NoProfile -File "%package_directory%\installer\install\install-core.ps1" -Mode package -Root "%package_directory%" %*
set "result=%errorlevel%"
:installer_entry_finish
if "%installer_entry_non_interactive%"=="1" exit /b %result%
if "%AGENT_WORK_KIT_INSTALLER_NESTED%"=="1" exit /b %result%
if defined CI exit /b %result%
if defined GITHUB_ACTIONS exit /b %result%
powershell.exe -NoProfile -Command "if (-not [Console]::IsInputRedirected) { [Console]::WriteLine('Installer exited (code %result%).'); [Console]::Write('Press Enter to exit...'); [void][Console]::ReadLine() }"
exit /b %result%
