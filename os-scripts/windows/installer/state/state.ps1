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

# Hash lines of the managed files below the given roots; $null when anything is a link or not a regular file/directory.
function Get-InstallerActualManifest {
    param([string] $SkillsRoot, [string] $BinDir)
    $lines = [System.Collections.Generic.List[string]]::new()
    $roots = @()
    foreach ($name in 'work', 'tooling', 'shared') { $roots += , @((Join-Path $SkillsRoot $name), "skills/$name") }
    $roots += , @($BinDir, 'cli/bin')
    foreach ($entry in $roots) {
        $base = $entry[0]
        if (-not (Test-Path -LiteralPath $base)) { continue }
        $rootItem = Get-Item -LiteralPath $base -Force
        if (-not $rootItem.PSIsContainer -or (Test-InstallerReparse $rootItem)) { return $null }
        $prefix = $rootItem.FullName.TrimEnd('\', '/')
        foreach ($item in @(Get-ChildItem -LiteralPath $base -Recurse -Force)) {
            if (Test-InstallerReparse $item) { return $null }
            if ($item.PSIsContainer) { continue }
            $relative = $item.FullName.Substring($prefix.Length).TrimStart('\', '/').Replace('\', '/')
            $label = "$($entry[1])/$relative"
            if (-not (Test-InstallerRelative $label)) { return $null }
            $lines.Add("$(Get-InstallerFileSha256 -Path $item.FullName)  $label")
        }
    }
    # Callers wrap the result in @(); $null (not an empty list) signals an unsafe tree.
    return @($lines | Sort-Object { $_ } -CaseSensitive)
}

function Test-InstallerManifestMatches {
    param([string] $SkillsRoot, [string] $BinDir, [string[]] $Entries)
    $actual = Get-InstallerActualManifest -SkillsRoot $SkillsRoot -BinDir $BinDir
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
    $keys = 'cli_install_dir,files,global_instructions_dir,host,rust_workspace_version,skill_install_dir,status,version'
    if ($value -isnot [pscustomobject] -or (@($value.PSObject.Properties.Name | Sort-Object { $_ } -CaseSensitive) -join ',') -cne $keys) { return $null }
    if ($value.host -cnotin @('codex', 'claude') -or $value.status -cne 'installed' -or
        -not (Test-InstallerVersion $value.version) -or -not (Test-InstallerVersion $value.rust_workspace_version)) { return $null }
    foreach ($dir in @($value.skill_install_dir, $value.cli_install_dir)) {
        if ($dir -isnot [string] -or -not [IO.Path]::IsPathRooted($dir)) { return $null }
    }
    if ($null -ne $value.global_instructions_dir -and
        ($value.global_instructions_dir -isnot [string] -or -not [IO.Path]::IsPathRooted($value.global_instructions_dir))) { return $null }
    $files = @($value.files)
    if ($files.Count -eq 0) { return $null }
    foreach ($file in $files) {
        if ($file -isnot [string] -or $file -cnotmatch '^[0-9a-f]{64}  (.+)$' -or -not (Test-InstallerRelative $Matches[1])) { return $null }
    }
    return $value
}

function Write-InstallerState {
    param([string] $Path, [string] $InstallerHost, [string] $Version, [string] $RustVersion, [string] $SkillsDir,
        [string] $CliDir, [AllowNull()][string] $GlobalDir, [string[]] $Files)
    $value = [ordered]@{
        host = $InstallerHost; version = $Version; rust_workspace_version = $RustVersion
        skill_install_dir = $SkillsDir; cli_install_dir = $CliDir
        global_instructions_dir = $(if ([string]::IsNullOrEmpty($GlobalDir)) { $null } else { $GlobalDir })
        status = 'installed'; files = @($Files)
    }
    [IO.File]::WriteAllText($Path, (($value | ConvertTo-Json -Depth 4) + "`n"), [Text.UTF8Encoding]::new($false))
}
