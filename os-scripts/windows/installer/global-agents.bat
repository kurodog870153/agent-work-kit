@echo off
setlocal EnableExtensions EnableDelayedExpansion
set "global_agents_source=%~1"

set "answer="
set /p "answer=Install Global AGENTS.md? [y/N] "
if errorlevel 1 goto input_error
if /i not "!answer!"=="y" goto skipped
if not exist "!global_agents_source!" (
    echo Error: Global AGENTS.md source not found: "!global_agents_source!".
    exit /b 1
)

:prompt_location
echo Global AGENTS.md installation location:
echo   1. Default Codex directory: "%USERPROFILE%\.codex"
echo   2. Custom directory
set "answer="
set /p "answer=Select an installation location [1]: "
if errorlevel 1 goto input_error
if not defined answer set "answer=1"
if "!answer!"=="1" (
    set "target_directory=%USERPROFILE%\.codex"
    goto location_selected
)
if not "!answer!"=="2" (
    echo Invalid selection. Please try again.
    goto prompt_location
)
set "target_directory="
set /p "target_directory=Enter the target directory: "
if errorlevel 1 goto input_error
if not defined target_directory (
    echo The directory cannot be empty. Please try again.
    goto prompt_location
)
if "!target_directory!"=="~" set "target_directory=%USERPROFILE%"
if "!target_directory:~0,2!"=="~/" set "target_directory=%USERPROFILE%\!target_directory:~2!"
if "!target_directory:~0,2!"=="~\" set "target_directory=%USERPROFILE%\!target_directory:~2!"

:location_selected
for %%I in ("!target_directory!") do set "target_directory=%%~fI"
if not exist "!target_directory!\." (
    set "answer="
    set /p "answer=The directory does not exist. Create it? [y/N] "
    if errorlevel 1 goto input_error
    if /i not "!answer!"=="y" goto skipped
    mkdir "!target_directory!"
    if errorlevel 1 (
        echo Error: failed to create Global AGENTS.md directory: "!target_directory!".
        exit /b 1
    )
)
set "target_file=!target_directory!\AGENTS.md"
if exist "!target_file!\NUL" (
    echo Error: Global AGENTS.md target is a directory.
    exit /b 1
)
if exist "!target_file!" (
    set "answer="
    set /p "answer=AGENTS.md already exists. Overwrite it? [y/N] "
    if errorlevel 1 goto input_error
    if /i not "!answer!"=="y" goto skipped
)

:choose_transaction_directory
set "transaction_directory=!target_directory!\.agents-install-!RANDOM!-!RANDOM!"
if exist "!transaction_directory!" goto choose_transaction_directory
mkdir "!transaction_directory!"
if errorlevel 1 (
    echo Error: failed to prepare Global AGENTS.md recovery directory.
    exit /b 1
)
set "staged=!transaction_directory!\prepared"
copy /Y "!global_agents_source!" "!staged!" >nul
if errorlevel 1 goto prepare_error
fc /b "!global_agents_source!" "!staged!" >nul
if errorlevel 1 goto prepare_error
if exist "!target_file!" (
    move "!target_file!" "!transaction_directory!\previous" >nul
    if errorlevel 1 (
        echo Error: failed to back up Global AGENTS.md; target was not changed.
        exit /b 1
    )
)
move "!staged!" "!target_file!" >nul
if errorlevel 1 (
    if exist "!transaction_directory!\previous" (
        move "!transaction_directory!\previous" "!target_file!" >nul
        if errorlevel 1 (
            echo Error: Global AGENTS.md restore failed; previous file remains in "!transaction_directory!\previous".
            exit /b 1
        )
    )
    echo Error: Global AGENTS.md replacement failed; previous file was restored. Recovery directory: "!transaction_directory!".
    exit /b 1
)
echo Global AGENTS.md installed in "!target_file!". Recovery directory: "!transaction_directory!".
exit /b 0

:prepare_error
echo Error: failed to prepare Global AGENTS.md; target was not changed. Recovery directory: "!transaction_directory!".
exit /b 1

:input_error
echo Error: installation input ended before a selection was completed.
exit /b 1

:skipped
echo Global AGENTS.md: Skipped.
exit /b 0
