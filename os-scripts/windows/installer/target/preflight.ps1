Set-StrictMode -Version Latest

# Read-only checks of the environment, sources and installation targets.
function Test-InstallerDirectoryPathSafe {
    param([string] $Path)
    $probe = $Path
    while (-not [string]::IsNullOrEmpty($probe)) {
        if (Test-Path -LiteralPath $probe) {
            $item = Get-Item -LiteralPath $probe -Force
            if (-not $item.PSIsContainer -or (Test-InstallerReparse $item)) { return $false }
        }
        $parent = Split-Path -Path $probe -Parent
        if ([string]::IsNullOrEmpty($parent) -or $parent -eq $probe) { break }
        $probe = $parent
    }
    return $true
}

function Test-InstallerPathOverlap {
    param([string] $A, [string] $B)
    $left = $A.TrimEnd('\', '/') + '\'; $right = $B.TrimEnd('\', '/') + '\'
    return $left.StartsWith($right, [StringComparison]::OrdinalIgnoreCase) -or $right.StartsWith($left, [StringComparison]::OrdinalIgnoreCase)
}

function Test-InstallerSourceTree {
    param([string] $Source)
    if (-not (Test-Path -LiteralPath $Source -PathType Container)) { return $false }
    if (Test-InstallerReparse (Get-Item -LiteralPath $Source -Force)) { return $false }
    $prefix = (Get-Item -LiteralPath $Source -Force).FullName.TrimEnd('\', '/')
    foreach ($item in @(Get-ChildItem -LiteralPath $Source -Recurse -Force)) {
        if (Test-InstallerReparse $item) { return $false }
        if ($item.PSIsContainer) { continue }
        if ($item.Extension -cnotin @('.md', '.yaml', '.yml')) { return $false }
        if (-not (Test-InstallerRelative $item.FullName.Substring($prefix.Length).TrimStart('\', '/').Replace('\', '/'))) { return $false }
    }
    return $true
}

function Assert-InstallerSources {
    param([string] $Root)
    foreach ($name in 'work', 'tooling') {
        $entry = Join-Path $Root "skills/$name/SKILL.md"
        if (-not (Test-InstallerSourceTree (Join-Path $Root "skills/$name")) -or -not (Test-Path -LiteralPath $entry -PathType Leaf) -or
            -not (Select-String -LiteralPath $entry -Pattern "^name: $name\s*$" -Quiet)) { throw "$name Skill source is missing or unsafe" }
    }
    $shared = Join-Path $Root 'skills/shared'
    if (-not (Test-InstallerSourceTree $shared) -or -not (Test-Path -LiteralPath (Join-Path $shared 'bootstrap.md') -PathType Leaf) -or
        (Test-Path -LiteralPath (Join-Path $shared 'SKILL.md'))) { throw 'Shared source is missing or unsafe' }
}

# Returns @{ Kind = new|managed|unsafe; Reason; State } without touching anything.
function Get-InstallerTargetInspection {
    param([string] $SkillsDir, [string] $CliDir)
    $statePath = Join-Path $SkillsDir 'install-state.json'
    if (Test-Path -LiteralPath $statePath) {
        $state = Read-InstallerState -Path $statePath
        if ($null -eq $state) { return @{ Kind = 'unsafe'; Reason = 'install-state.json is invalid'; State = $null } }
        if (-not (Test-InstallerPathEqual $state.skill_install_dir $SkillsDir) -or -not (Test-InstallerPathEqual $state.cli_install_dir $CliDir)) {
            return @{ Kind = 'unsafe'; Reason = 'install state belongs to another Skills or CLI directory'; State = $null }
        }
        if (-not (Test-InstallerManifestMatches -SkillsRoot $SkillsDir -BinDir (Join-Path $CliDir 'bin') -Entries @($state.files))) {
            return @{ Kind = 'unsafe'; Reason = 'installed content differs from the recorded installation'; State = $null }
        }
        return @{ Kind = 'managed'; Reason = ''; State = $state }
    }
    $paths = @('work', 'tooling', 'shared' | ForEach-Object { Join-Path $SkillsDir $_ }) + @(Join-Path $CliDir 'bin')
    foreach ($path in $paths) {
        if (-not (Test-Path -LiteralPath $path)) { continue }
        $item = Get-Item -LiteralPath $path -Force
        if (-not $item.PSIsContainer -or (Test-InstallerReparse $item) -or @(Get-ChildItem -LiteralPath $path -Force).Count -gt 0) {
            return @{ Kind = 'unsafe'; Reason = "existing content without a valid install state: $path"; State = $null }
        }
    }
    return @{ Kind = 'new'; Reason = ''; State = $null }
}

function Test-InstallerBinaryInUse {
    param([string] $Binary)
    if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { return $false }
    $full = [IO.Path]::GetFullPath($Binary)
    foreach ($process in @(Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($Binary)) -ErrorAction SilentlyContinue)) {
        try { if ([string]::Equals($process.Path, $full, [StringComparison]::OrdinalIgnoreCase)) { return $true } } catch { }
    }
    try {
        $stream = [IO.File]::Open($full, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
        $stream.Dispose()
        return $false
    } catch [IO.IOException] { return $true }
}
