@echo off
chcp 65001 >nul
setlocal EnableExtensions DisableDelayedExpansion

set "codex_directory=%USERPROFILE%\.codex"

echo Codex directory: "%codex_directory%"
echo This will permanently delete:
echo   sessions
echo   archived_sessions
echo   generated_images
echo   session_index.jsonl
echo   history.jsonl
echo   state_5.sqlite
echo   state_5.sqlite-shm
echo   state_5.sqlite-wal
echo ChatGPT cache directories found:
set "has_data=0"
call :chatgpt_caches list
echo ChatGPT login and settings will be preserved.
echo Close Codex and ChatGPT before continuing.

if exist "%codex_directory%\" (
    set "has_data=1"
) else (
    echo No Codex directory was found. Only ChatGPT caches will be considered.
)

if "%has_data%"=="0" (
    echo No Codex directory or supported ChatGPT cache directories were found. Nothing to clean.
    pause
    exit /b 0
)

choice /c YN /n /m "Permanently delete the listed Codex data and ChatGPT caches? [Y/N] "
if errorlevel 255 (
    echo Error: confirmation could not be read. Nothing was cleaned.
    pause
    exit /b 1
)
if errorlevel 2 (
    echo Operation cancelled.
    pause
    exit /b 0
)
if not errorlevel 1 (
    echo Operation cancelled.
    pause
    exit /b 0
)

set "cleanup_failed=0"

rmdir /s /q "%codex_directory%\sessions" 2>nul
if exist "%codex_directory%\sessions\" set "cleanup_failed=1"
rmdir /s /q "%codex_directory%\archived_sessions" 2>nul
if exist "%codex_directory%\archived_sessions\" set "cleanup_failed=1"
rmdir /s /q "%codex_directory%\generated_images" 2>nul
if exist "%codex_directory%\generated_images\" set "cleanup_failed=1"

del /f /q "%codex_directory%\session_index.jsonl" 2>nul
if exist "%codex_directory%\session_index.jsonl" set "cleanup_failed=1"
del /f /q "%codex_directory%\history.jsonl" 2>nul
if exist "%codex_directory%\history.jsonl" set "cleanup_failed=1"
del /f /q "%codex_directory%\state_5.sqlite" 2>nul
if exist "%codex_directory%\state_5.sqlite" set "cleanup_failed=1"
del /f /q "%codex_directory%\state_5.sqlite-shm" 2>nul
if exist "%codex_directory%\state_5.sqlite-shm" set "cleanup_failed=1"
del /f /q "%codex_directory%\state_5.sqlite-wal" 2>nul
if exist "%codex_directory%\state_5.sqlite-wal" set "cleanup_failed=1"

call :chatgpt_caches clean

if "%cleanup_failed%"=="1" (
    echo Error: one or more Codex data files or ChatGPT cache directories could not be removed.
    pause
    exit /b 1
)

echo The listed Codex data and ChatGPT caches were cleaned successfully.
pause
exit /b 0

:chatgpt_caches
if defined APPDATA (
    call :cache_root "%~1" "%APPDATA%\ChatGPT"
    call :cache_root "%~1" "%APPDATA%\Codex"
)
if defined LOCALAPPDATA (
    for /d %%P in ("%LOCALAPPDATA%\Packages\OpenAI.ChatGPT-Desktop_*" "%LOCALAPPDATA%\Packages\OpenAI.Codex_*") do (
        call :cache_root "%~1" "%%~fP\LocalCache\Roaming\ChatGPT"
        call :cache_root "%~1" "%%~fP\LocalCache\Roaming\Codex"
    )
)
exit /b 0

:cache_root
for %%C in ("Cache" "Code Cache" "GPUCache" "DawnCache" "GrShaderCache" "ShaderCache") do (
    call :cache_directory "%~1" "%~2\%%~C"
)
exit /b 0

:cache_directory
if not exist "%~2\" exit /b 0
if "%~1"=="list" (
    echo   "%~2"
    set "has_data=1"
    exit /b 0
)
rmdir /s /q "%~2" 2>nul
if exist "%~2\" set "cleanup_failed=1"
exit /b 0
