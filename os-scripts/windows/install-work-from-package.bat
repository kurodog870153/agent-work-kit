@echo off
chcp 65001 <nul >nul
setlocal DisableDelayedExpansion
for %%I in ("%~dp0.") do set "package_directory=%%~fI"
powershell.exe -NoProfile -File "%package_directory%\installer\package\verify-integrity.ps1" -PackageRoot "%package_directory%"
if errorlevel 1 exit /b 1
if /I not "%PROCESSOR_ARCHITECTURE%"=="AMD64" (
    echo Error: this release package requires Windows x64.
    exit /b 1
)
findstr /c:"x86_64-pc-windows-msvc" "%package_directory%\release-info.json" >nul
if errorlevel 1 (
    echo Error: release package target is not Windows x64 MSVC.
    exit /b 1
)
if not exist "%package_directory%\bin\work.exe" (
    echo Error: packaged Work binary is missing.
    exit /b 1
)
"%package_directory%\bin\work.exe" --help >nul 2>nul
if errorlevel 1 exit /b 1
powershell.exe -NoProfile -File "%package_directory%\installer\install\install-core.ps1" -Mode package -Root "%package_directory%" %*
exit /b %errorlevel%
