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

# Validate the exact absolute target and Directory Junction type; SymbolicLink is never accepted.
function Test-InstallerHostLink {
    param([string] $Name, [string] $InstallDir, [string] $HostSkillsDir)
    $link = Join-Path $HostSkillsDir $Name
    $expected = Join-Path $InstallDir "skills/$Name"
    if (-not (Test-InstallerExists $link)) { return $false }
    $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
    if ($null -eq $item -or -not (Test-InstallerReparse $item) -or $item.LinkType -cne 'Junction') { return $false }
    $targets = @($item.Target)
    if ($targets.Count -ne 1 -or -not [IO.Path]::IsPathRooted([string] $targets[0]) -or
        -not (Test-InstallerPathEqual ([string] $targets[0]) $expected)) { return $false }
    return (Test-Path -LiteralPath $link -PathType Container)
}

function Get-InstallerTargetInspection {
    param([string] $InstallDir, [string] $HostSkillsDir, [string] $InstallerHost)
    $statePath = Join-Path $InstallDir 'install-state.json'
    if (Test-InstallerExists $statePath) {
        $state = Read-InstallerState -Path $statePath
        if ($null -eq $state) { return @{ Kind = 'unsafe'; Reason = 'install-state.json is invalid'; State = $null } }
        if (-not (Test-InstallerPathEqual $state.install_dir $InstallDir) -or $state.host -cne $InstallerHost) {
            return @{ Kind = 'unsafe'; Reason = 'install state belongs to another Host or installation directory'; State = $null }
        }
        if (-not (Test-InstallerManifestMatches -InstallDir $InstallDir -Entries @($state.files))) {
            return @{ Kind = 'unsafe'; Reason = 'installed content differs from the recorded installation'; State = $null }
        }
        foreach ($name in 'work', 'tooling') {
            if (-not (Test-InstallerHostLink -Name $name -InstallDir $InstallDir -HostSkillsDir $HostSkillsDir)) {
                return @{ Kind = 'unsafe'; Reason = "missing or invalid Host discovery link: $name"; State = $null }
            }
        }
        return @{ Kind = 'managed'; Reason = ''; State = $state }
    }
    if (Test-Path -LiteralPath $InstallDir -PathType Container) {
        if (@(Get-ChildItem -LiteralPath $InstallDir -Force).Count -gt 0) {
            return @{ Kind = 'unsafe'; Reason = "existing content without a valid install state: $InstallDir"; State = $null }
        }
    }
    foreach ($name in 'work', 'tooling') {
        $link = Join-Path $HostSkillsDir $name
        if (Test-InstallerExists $link) {
            return @{ Kind = 'unsafe'; Reason = "Host discovery entry is occupied: $link"; State = $null }
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
