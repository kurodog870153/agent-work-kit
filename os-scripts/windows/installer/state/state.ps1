Set-StrictMode -Version Latest

# The single authoritative install state: one fixed set of keys, parsed strictly (same contract as macOS).
function Test-InstallerVersion { param($Value) return ($Value -is [string]) -and ($Value -cmatch '^[0-9]+\.[0-9]+\.[0-9]+$') }
function Test-InstallerReparse { param($Item) return ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }
function Test-InstallerRelative {
    param([string] $Value)
    return ($Value -cmatch '^[A-Za-z0-9._/-]+$') -and ("/$Value/" -notmatch '/\.\.?/') -and -not $Value.StartsWith('/') -and -not $Value.EndsWith('/') -and -not $Value.Contains('//')
}
function Get-InstallerFileSha256 {
    param([Parameter(Mandatory = $true)][string] $Path)
    # Pure .NET: Get-FileHash lives in Microsoft.PowerShell.Utility, which is unavailable in restricted module environments.
    $stream = [IO.File]::OpenRead((Get-Item -LiteralPath $Path -Force).FullName)
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { return [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
        finally { $hasher.Dispose() }
    } finally { $stream.Dispose() }
}
function Test-InstallerPathEqual { param([string] $A, [string] $B) return [string]::Equals($A.TrimEnd('\', '/'), $B.TrimEnd('\', '/'), [StringComparison]::OrdinalIgnoreCase) }

# Hash lines of managed files in a single install root, never the Host discovery parent.
function Get-InstallerActualManifest {
    param([string] $InstallDir, [switch] $Staged)
    $lines = [System.Collections.Generic.List[string]]::new()
    $sourceRoot = if ($Staged) { $InstallDir } else { Join-Path $InstallDir 'skills' }
    if (-not $Staged) {
        if (-not (Test-InstallerDirectoryPathSafe $sourceRoot) -or -not (Test-Path -LiteralPath $sourceRoot -PathType Container)) { return $null }
        if (@(Get-ChildItem -LiteralPath $sourceRoot -Force | Where-Object { $_.Name -cnotin @('work', 'tooling', 'shared') }).Count -ne 0) { return $null }
    }
    foreach ($file in @((Join-Path $sourceRoot 'work/instructions/instructions.md'), (Join-Path $sourceRoot 'tooling/instructions/instructions.md'),
                        (Join-Path $sourceRoot 'shared/bootstrap.md'), (Join-Path $InstallDir 'bin/work.exe'), (Join-Path $InstallDir 'bin/skill.exe'))) {
        if (-not (Test-Path -LiteralPath $file -PathType Leaf) -or (Test-InstallerReparse (Get-Item -LiteralPath $file -Force))) { return $null }
    }
    foreach ($name in 'work', 'tooling', 'shared', 'bin') {
        $prefix = if ($name -eq 'bin') { 'bin' } else { "skills/$name" }
        $source = if ($name -eq 'bin' -or $Staged) { $name } else { "skills/$name" }
        $base = Join-Path $InstallDir $source
        if (-not (Test-InstallerExists $base)) { continue }
        $rootItem = Get-Item -LiteralPath $base -Force
        if (-not $rootItem.PSIsContainer -or (Test-InstallerReparse $rootItem)) { return $null }
        $rootPath = $rootItem.FullName.TrimEnd('\', '/')
        foreach ($item in @(Get-ChildItem -LiteralPath $base -Recurse -Force)) {
            if (Test-InstallerReparse $item) { return $null }
            if ($item.PSIsContainer) { continue }
            $relative = $item.FullName.Substring($rootPath.Length).TrimStart('\', '/').Replace('\', '/')
            $label = "$prefix/$relative"
            if (-not (Test-InstallerRelative $label) -or $label -clike 'skills/*/SKILL.md') { return $null }
            $lines.Add("$(Get-InstallerFileSha256 -Path $item.FullName)  $label")
        }
    }
    foreach ($name in 'work', 'tooling') {
        if ($Staged) { $file = Join-Path $InstallDir "host-$name" }
        else {
            $dir = Join-Path $script:InstallerHostSkillsDir $name
            if (-not (Test-InstallerDirectoryPathSafe $dir) -or -not (Test-Path -LiteralPath $dir -PathType Container)) { return $null }
            $file = Join-Path $dir 'SKILL.md'
            $items = @(Get-ChildItem -LiteralPath $dir -Force)
            if ($items.Count -ne 1 -or $items[0].Name -cne 'SKILL.md') { return $null }
        }
        if (-not (Test-Path -LiteralPath $file -PathType Leaf) -or (Test-InstallerReparse (Get-Item -LiteralPath $file -Force))) { return $null }
        $lines.Add("$(Get-InstallerFileSha256 -Path $file)  host-skills/$name/SKILL.md")
    }
    return @($lines | Sort-Object { $_ } -CaseSensitive)
}

function Test-InstallerManifestMatches {
    param([string] $InstallDir, [string[]] $Entries)
    $actual = Get-InstallerActualManifest -InstallDir $InstallDir
    if ($null -eq $actual) { return $false }
    $expected = @($Entries | Where-Object { $_ -cnotmatch '^[0-9a-f]{64}  global/' } | Sort-Object { $_ } -CaseSensitive)
    return ($expected -join "`n") -ceq (@($actual) -join "`n")
}

# Returns the state object, or $null when the file deviates from the contract.
function Read-InstallerState {
    param([string] $Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    if (Test-InstallerReparse (Get-Item -LiteralPath $Path -Force)) { return $null }
    try { $value = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($Path)) -ErrorAction Stop } catch { return $null }
    $keys = 'files,global_instructions_dir,host,install_dir,rust_workspace_version,status,version'
    if ($value -isnot [pscustomobject] -or (@($value.PSObject.Properties.Name | Sort-Object { $_ } -CaseSensitive) -join ',') -cne $keys) { return $null }
    if ($value.host -cnotin @('codex', 'claude') -or $value.status -cne 'installed' -or
        -not (Test-InstallerVersion $value.version) -or -not (Test-InstallerVersion $value.rust_workspace_version)) { return $null }
    if ($value.install_dir -isnot [string] -or -not (Test-InstallerAbsolutePath $value.install_dir)) { return $null }
    if ($null -ne $value.global_instructions_dir -and
        ($value.global_instructions_dir -isnot [string] -or -not (Test-InstallerAbsolutePath $value.global_instructions_dir))) { return $null }
    $files = @($value.files)
    if ($files.Count -eq 0) { return $null }
    $seen = @()
    $globalName = if ($value.host -ceq 'claude') { 'CLAUDE.md' } else { 'AGENTS.md' }
    $globalCount = 0
    foreach ($file in $files) {
        if ($file -isnot [string] -or $file -cnotmatch '^[0-9a-f]{64}  (.+)$') { return $null }
        $relative = $Matches[1]
        if (-not (Test-InstallerRelative $relative) -or $seen -ccontains $relative) { return $null }
        $seen += $relative
        if ($relative -ceq "global/$globalName") { $globalCount++ }
        elseif ($relative -cnotmatch '^(skills/(work|tooling|shared)/.+|bin/.+|host-skills/(work|tooling)/SKILL\.md)$') { return $null }
    }
    if (($null -eq $value.global_instructions_dir -and $globalCount -ne 0) -or
        ($null -ne $value.global_instructions_dir -and $globalCount -ne 1)) { return $null }
    foreach ($name in 'work', 'tooling') {
        if (@($files | Where-Object { $_ -cmatch "^[0-9a-f]{64}  host-skills/$name/SKILL\.md$" }).Count -ne 1) { return $null }
    }
    return $value
}

function Write-InstallerState {
    param([string] $Path, [string] $InstallerHost, [string] $Version, [string] $RustVersion, [string] $InstallDir,
        [AllowNull()][string] $GlobalDir, [string[]] $Files)
    $value = [ordered]@{
        host = $InstallerHost; version = $Version; rust_workspace_version = $RustVersion
        install_dir = $InstallDir
        global_instructions_dir = $(if ([string]::IsNullOrEmpty($GlobalDir)) { $null } else { $GlobalDir })
        status = 'installed'; files = @($Files)
    }
    [IO.File]::WriteAllText($Path, (($value | ConvertTo-Json -Depth 4) + "`n"), [Text.UTF8Encoding]::new($false))
}
