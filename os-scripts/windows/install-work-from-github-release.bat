@echo off
chcp 65001 <nul >nul
setlocal DisableDelayedExpansion
if "%~1"=="--help" if "%~2"=="" (
    echo Usage: install-work-from-github-release.bat [installer options]
    echo Downloads the fixed Windows x64 Release, verifies SHA-256, then runs its Package installer.
    exit /b 0
)
if "%~1"=="-h" if "%~2"=="" (
    echo Usage: install-work-from-github-release.bat [installer options]
    echo Downloads the fixed Windows x64 Release, verifies SHA-256, then runs its Package installer.
    exit /b 0
)
set "scratch=%TEMP%\work-release-%RANDOM%%RANDOM%%RANDOM%"
if exist "%scratch%" exit /b 1
mkdir "%scratch%" || exit /b 1
set "marker_line="
for /f "tokens=1 delims=:" %%N in ('findstr /n /b /c:"# POWERSHELL_PAYLOAD" "%~f0"') do set "marker_line=%%N"
if not defined marker_line (
    rmdir "%scratch%"
    exit /b 1
)
more +%marker_line% "%~f0" > "%scratch%\download.ps1"
powershell.exe -NoProfile -File "%scratch%\download.ps1" -Scratch "%scratch%" %*
set "result=%errorlevel%"
if exist "%scratch%\download.ps1" del /f /q "%scratch%\download.ps1" >nul 2>nul
if exist "%scratch%" rmdir /s /q "%scratch%" >nul 2>nul
exit /b %result%
# POWERSHELL_PAYLOAD
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($args.Count -lt 2 -or $args[0] -cne '-Scratch') { exit 2 }
$scratch = [string] $args[1]
$cli = @()
if ($args.Count -gt 2) { $cli = [string[]] $args[2..($args.Count - 1)] }
$releaseTag = '__WORK_RELEASE_TAG__'
$archiveName = "work-$releaseTag-windows-x64-msvc.zip"
$base = "https://github.com/kurodog870153/agent-work-kit/releases/download/$releaseTag"
function Clear-ReleaseStaleScratch {
    $parent = Split-Path -Path $scratch -Parent
    foreach ($folder in @(Get-ChildItem -LiteralPath $parent -Directory -Force -ErrorAction SilentlyContinue)) {
        if ($folder.FullName -ceq $scratch -or $folder.Name -cnotmatch '^work-release-[0-9]+$' -or
            ($folder.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        $marker = Join-Path $folder.FullName '.work-release-owner'
        if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { continue }
        $markerItem = Get-Item -LiteralPath $marker -Force
        if (($markerItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        $record = [IO.File]::ReadAllText($marker).Trim()
        if ($record -cnotmatch '^work-release-v1\|([0-9]+)$') { continue }
        if (Get-Process -Id ([int]$Matches[1]) -ErrorAction SilentlyContinue) { continue }
        try { $lock = [IO.File]::Open($marker, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
        catch { continue }
        try {
            $allowed = @('.work-release-owner', 'download.ps1', 'SHA256SUMS.txt', $archiveName, 'package')
            $safe = $true
            $stack = [System.Collections.Generic.Stack[object]]::new()
            foreach ($item in @(Get-ChildItem -LiteralPath $folder.FullName -Force)) {
                if ($item.Name -notin $allowed) { $safe = $false; break }
                $stack.Push($item)
            }
            while ($safe -and $stack.Count -gt 0) {
                $item = $stack.Pop()
                if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { $safe = $false; break }
                if ($item.PSIsContainer) {
                    foreach ($child in @(Get-ChildItem -LiteralPath $item.FullName -Force)) { $stack.Push($child) }
                } elseif ($item.FullName -cne $marker) {
                    try {
                        $probe = [IO.File]::Open($item.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
                        $probe.Dispose()
                    } catch { $safe = $false }
                }
            }
        } finally { $lock.Dispose() }
        if (-not $safe) { Write-Warning "Downloader temporary directory retained: $($folder.FullName)"; continue }
        try {
            foreach ($name in @('package', $archiveName, 'SHA256SUMS.txt', 'download.ps1', '.work-release-owner')) {
                $itemPath = Join-Path $folder.FullName $name
                if (Test-Path -LiteralPath $itemPath) {
                    Remove-Item -LiteralPath $itemPath -Recurse -Force -ErrorAction Stop
                }
            }
            Remove-Item -LiteralPath $folder.FullName -Force -ErrorAction Stop
            if ($cli -contains '--verbose') { Write-Output 'Downloader: removed unused temporary directory' }
        } catch { Write-Warning "Downloader temporary directory awaits cleanup: $($folder.FullName). $_" }
    }
}
function Get-ReleaseAsset {
    param([string] $Name)
    $path = Join-Path $scratch $Name
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$Name" -OutFile $path -TimeoutSec 120 -MaximumRedirection 3 -ErrorAction Stop | Out-Null
        return $path
    } catch {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
        throw "download failed: $Name. $_"
    }
}
try {
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'this downloader requires Windows x64' }
    Clear-ReleaseStaleScratch
    $ownerPath = Join-Path $scratch '.work-release-owner'
    [IO.File]::WriteAllText($ownerPath, "work-release-v1|$PID")
    $ownerHandle = [IO.File]::Open($ownerPath, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    $checksumPath = Get-ReleaseAsset -Name 'SHA256SUMS.txt'
    $archivePath = Get-ReleaseAsset -Name $archiveName
    $expected = $null
    foreach ($line in [IO.File]::ReadAllLines($checksumPath, [Text.Encoding]::ASCII)) {
        if ($line -cnotmatch '^([0-9a-f]{64})  ([A-Za-z0-9._-]+)$') { throw 'invalid outer checksum file' }
        if ($Matches[2] -ceq $archiveName) {
            if ($null -ne $expected) { throw 'duplicate ZIP checksum' }
            $expected = $Matches[1]
        }
    }
    if ($null -eq $expected) { throw 'ZIP checksum is missing' }
    $stream = [IO.File]::OpenRead($archivePath)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $actual = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
        finally { $sha.Dispose() }
    } finally { $stream.Dispose() }
    if ($actual -cne $expected) { throw 'ZIP SHA-256 mismatch' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($archivePath)
    try {
        if ($archive.Entries.Count -eq 0) { throw 'empty ZIP' }
        $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($entry in $archive.Entries) {
            $name = $entry.FullName
            if ($name -cnotmatch '^[A-Za-z0-9._/-]+$' -or $name.StartsWith('/') -or $name.EndsWith('/') -or
                $name.Contains('//') -or $name -match '(^|/)(\.|\.\.)(/|$)' -or -not $seen.Add($name)) {
                throw "unsafe ZIP member: $name"
            }
            $mode = ($entry.ExternalAttributes -shr 16) -band 0xFFFF
            if (($mode -band 0xF000) -ne 0x8000) { throw "non-regular ZIP member: $name" }
        }
    } finally { $archive.Dispose() }
    $package = Join-Path $scratch 'package'
    [IO.Compression.ZipFile]::ExtractToDirectory($archivePath, $package)
    $entryPath = Join-Path $package 'install-work-from-package.bat'
    if (-not (Test-Path -LiteralPath $entryPath -PathType Leaf)) { throw 'Package entry is missing' }
    $infoPath = Join-Path $package 'release-info.json'
    if (-not (Test-Path -LiteralPath $infoPath -PathType Leaf)) { throw 'Package release metadata is missing' }
    $packageInfo = Get-Content -LiteralPath $infoPath -Raw | ConvertFrom-Json -ErrorAction Stop
    if ($packageInfo.tag -cne $releaseTag) { throw 'Package Release tag does not match Downloader' }
    & $entryPath @cli
    exit $LASTEXITCODE
} catch {
    [Console]::Error.WriteLine("Error: $($_.Exception.Message)")
    exit 1
}
