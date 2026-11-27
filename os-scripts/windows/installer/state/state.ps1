Set-StrictMode -Version Latest

# The single authoritative install state: one fixed set of keys, parsed strictly (same contract as macOS).
function Test-InstallerVersion { param($Value) return ($Value -is [string]) -and ($Value -cmatch '\A[0-9]+\.[0-9]+\.[0-9]+\z') }
function Test-InstallerReparse { param($Item) return ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }
function Test-InstallerRelative {
    param([string] $Value)
    return ($Value -cmatch '\A[A-Za-z0-9._/-]+\z') -and ("/$Value/" -notmatch '/\.\.?/') -and -not $Value.StartsWith('/') -and -not $Value.EndsWith('/') -and -not $Value.Contains('//')
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

# Validate the narrow JSON grammar before ConvertFrom-Json (which can collapse duplicate keys).
# String tokens implement JSON escaping; the grammar allows only the root and file record shapes.
function Test-InstallerStateJson {
    param([string] $Text)
    $ws = '[ \t\r\n]*'
    $str = '"(?:[^"\\\x00-\x1f]|\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4}))*"'
    $pair = "$str$ws`:$ws$str"
    $record = "\{$ws$pair(?:$ws,$ws$pair)*$ws\}"
    $array = "\[$ws(?:$record(?:$ws,$ws$record)*)?$ws\]"
    $rootPair = "$str$ws`:$ws(?:$str|null|$array)"
    $grammar = "\A$ws\{$ws$rootPair(?:$ws,$ws$rootPair)*$ws\}$ws\z"
    if (-not [regex]::IsMatch($Text, $grammar, [Text.RegularExpressions.RegexOptions]::None, [TimeSpan]::FromSeconds(5))) { return $false }
    $depth = 0; $rootNames = @(); $recordNames = @()
    foreach ($token in [regex]::Matches($Text, "$str|[{}]")) {
        if ($token.Value -ceq '{') { $depth++; $recordNames = @(); continue }
        if ($token.Value -ceq '}') { $depth--; continue }
        if ($Text.Substring($token.Index + $token.Length) -notmatch '^[ \t\r\n]*:') { continue }
        # Decode escape sequences in keys before checking duplicates.
        $key = (ConvertFrom-Json -InputObject ('{"key":' + $token.Value + '}') -ErrorAction Stop).key
        if ($depth -eq 1) {
            if ($key -cnotin @('files','global_instructions_dir','host','install_dir','rust_workspace_version','status','version') -or $rootNames -ccontains $key) { return $false }
            $rootNames += $key
        } else {
            if ($key -cnotin @('path','sha256') -or $recordNames -ccontains $key) { return $false }
            $recordNames += $key
        }
    }
    return $true
}

# Returns the state object, or $null when the file deviates from the contract.
function Read-InstallerState {
    param([string] $Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    if (Test-InstallerReparse (Get-Item -LiteralPath $Path -Force)) { return $null }
    try {
        $text = [IO.File]::ReadAllText($Path, [Text.UTF8Encoding]::new($false, $true))
        if (-not (Test-InstallerStateJson $text)) { return $null }
        $value = ConvertFrom-Json -InputObject $text -ErrorAction Stop
    } catch { return $null }
    $keys = @('files','global_instructions_dir','host','install_dir','rust_workspace_version','status','version')
    if ($value -isnot [pscustomobject] -or @($value.PSObject.Properties).Count -ne $keys.Count) { return $null }
    foreach ($key in $keys) { if ($value.PSObject.Properties.Name -cnotcontains $key) { return $null } }
    if ($value.host -isnot [string] -or $value.host -cnotin @('codex', 'claude') -or
        $value.status -isnot [string] -or $value.status -cne 'installed' -or
        -not (Test-InstallerVersion $value.version) -or -not (Test-InstallerVersion $value.rust_workspace_version)) { return $null }
    if ($value.install_dir -isnot [string] -or $value.install_dir -match '[\x00-\x1f\x7f]' -or -not (Test-InstallerAbsolutePath $value.install_dir)) { return $null }
    if ($null -ne $value.global_instructions_dir -and
        ($value.global_instructions_dir -isnot [string] -or $value.global_instructions_dir -match '[\x00-\x1f\x7f]' -or -not (Test-InstallerAbsolutePath $value.global_instructions_dir))) { return $null }
    if ($value.files -isnot [array] -or $value.files.Count -eq 0) { return $null }
    $seen = @()
    $globalName = if ($value.host -ceq 'claude') { 'CLAUDE.md' } else { 'AGENTS.md' }
    $globalCount = 0
    foreach ($file in $value.files) {
        if ($file -isnot [pscustomobject] -or @($file.PSObject.Properties).Count -ne 2 -or
            $file.PSObject.Properties.Name -cnotcontains 'path' -or $file.PSObject.Properties.Name -cnotcontains 'sha256' -or
            $file.path -isnot [string] -or $file.sha256 -isnot [string] -or $file.sha256 -cnotmatch '\A[0-9a-f]{64}\z') { return $null }
        $relative = $file.path
        if (-not (Test-InstallerRelative $relative) -or $seen -ccontains $relative) { return $null }
        $seen += $relative
        if ($relative -ceq "global/$globalName") { $globalCount++ }
        elseif ($relative -cnotmatch '\A(?:skills/(work|tooling|shared)/.+|bin/.+|host-skills/(work|tooling)/SKILL\.md)\z') { return $null }
    }
    if (($null -eq $value.global_instructions_dir -and $globalCount -ne 0) -or
        ($null -ne $value.global_instructions_dir -and $globalCount -ne 1)) { return $null }
    foreach ($name in 'work', 'tooling') {
        if ($seen -cnotcontains "host-skills/$name/SKILL.md") { return $null }
    }
    return $value
}

# Preserve the internal checksum manifest used by transaction and filesystem comparisons.
function ConvertTo-InstallerManifestLines {
    param([object[]] $Files)
    foreach ($file in $Files) { "$($file.sha256)  $($file.path)" }
}

function Write-InstallerState {
    param([string] $Path, [string] $InstallerHost, [string] $Version, [string] $RustVersion, [string] $InstallDir,
        [AllowNull()][string] $GlobalDir, [string[]] $Files)
    # Windows PowerShell 5.1 ConvertTo-Json indents differently from macOS.
    # Use its JSON string escaping, but write the fixed two-space layout explicitly.
    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add('{')
    $lines.Add('  "host": ' + (ConvertTo-Json -InputObject $InstallerHost -Compress) + ',')
    $lines.Add('  "version": ' + (ConvertTo-Json -InputObject $Version -Compress) + ',')
    $lines.Add('  "rust_workspace_version": ' + (ConvertTo-Json -InputObject $RustVersion -Compress) + ',')
    $lines.Add('  "install_dir": ' + (ConvertTo-Json -InputObject $InstallDir -Compress) + ',')
    $globalJson = if ([string]::IsNullOrEmpty($GlobalDir)) { 'null' } else { ConvertTo-Json -InputObject $GlobalDir -Compress }
    $lines.Add('  "global_instructions_dir": ' + $globalJson + ',')
    $lines.Add('  "status": "installed",')
    $lines.Add('  "files": [')
    [string[]] $sorted = @(foreach ($entry in $Files) { $entry.Substring(66) + '  ' + $entry.Substring(0, 64) })
    [Array]::Sort($sorted, [StringComparer]::Ordinal)
    for ($index = 0; $index -lt $sorted.Count; $index++) {
        $entry = $sorted[$index]
        $separator = $entry.IndexOf('  ', [StringComparison]::Ordinal)
        $lines.Add('    {')
        $lines.Add('      "path": "' + $entry.Substring(0, $separator) + '",')
        $lines.Add('      "sha256": "' + $entry.Substring($separator + 2) + '"')
        $lines.Add($(if ($index -lt $sorted.Count - 1) { '    },' } else { '    }' }))
    }
    $lines.Add('  ]')
    $lines.Add('}')
    [IO.File]::WriteAllText($Path, (($lines -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
}
