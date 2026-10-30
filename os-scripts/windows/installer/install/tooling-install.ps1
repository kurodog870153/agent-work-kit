Set-StrictMode -Version Latest

function Test-InstallerToolingSource {
    param([string] $Source)
    if (-not (Test-Path -LiteralPath $Source -PathType Container) -or
        ((Get-Item -LiteralPath $Source -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    foreach ($required in @('SKILL.md', 'references/instruction-loading.md', 'references/instructions/instructions.md')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Source $required) -PathType Leaf)) { return $false }
    }
    $stack = [System.Collections.Generic.Stack[object]]::new()
    foreach ($entry in @(Get-ChildItem -LiteralPath $Source -Force)) { $stack.Push($entry) }
    while ($stack.Count -gt 0) {
        $entry = $stack.Pop()
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
        if ($entry.PSIsContainer) {
            foreach ($child in @(Get-ChildItem -LiteralPath $entry.FullName -Force)) { $stack.Push($child) }
        } elseif ($entry.Extension -cnotin @('.md', '.yaml', '.yml')) { return $false }
    }
    return $true
}

function Test-InstallerToolingSubset {
    param([string] $Path, [string] $Source)
    if (-not (Test-Path -LiteralPath $Path -PathType Container) -or
        ((Get-Item -LiteralPath $Path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    $stack = [System.Collections.Generic.Stack[object]]::new()
    foreach ($entry in @(Get-ChildItem -LiteralPath $Path -Force)) { $stack.Push($entry) }
    while ($stack.Count -gt 0) {
        $entry = $stack.Pop()
        $relative = $entry.FullName.Substring($Path.TrimEnd('\', '/').Length + 1)
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
        if ($relative -eq 'install-state.json') {
            if ($entry.PSIsContainer) { return $false }
        } elseif ($relative.Replace('\', '/') -eq 'scripts') {
            if (-not $entry.PSIsContainer) { return $false }
        } elseif ($relative.Replace('\', '/') -eq 'scripts/tooling.exe') {
            if ($entry.PSIsContainer) { return $false }
        } else {
            $sourcePath = Join-Path $Source $relative
            if ($entry.PSIsContainer) {
                if (-not (Test-Path -LiteralPath $sourcePath -PathType Container)) { return $false }
            } elseif (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) { return $false }
        }
        if ($entry.PSIsContainer) {
            foreach ($child in @(Get-ChildItem -LiteralPath $entry.FullName -Force)) { $stack.Push($child) }
        }
    }
    return $true
}

function Inspect-InstallerToolingTarget {
    param([string] $Target, [string] $Source)
    $unsafe = { param($Reason) [pscustomobject]@{ Kind = 'unsafe'; Reason = $Reason; Version = $null } }
    if (-not [IO.Path]::IsPathRooted($Target) -or [IO.Path]::GetFileName($Target) -cne 'tooling') {
        return (& $unsafe 'target must be an absolute tooling directory')
    }
    if (-not (Test-InstallerDirectoryPathSafe -Path $Target)) { return (& $unsafe 'unsafe target path') }
    if (-not (Test-Path -LiteralPath $Target)) { return [pscustomobject]@{ Kind = 'new'; Reason = ''; Version = $null } }
    if (-not (Test-Path -LiteralPath $Target -PathType Container)) { return (& $unsafe 'target is not a directory') }
    if (@(Get-ChildItem -LiteralPath $Target -Force).Count -eq 0) {
        return [pscustomobject]@{ Kind = 'new'; Reason = ''; Version = $null }
    }
    if (-not (Test-InstallerToolingSubset -Path $Target -Source $Source)) { return (& $unsafe 'unknown or unsafe Tooling entry') }
    foreach ($required in @('SKILL.md', 'references/instruction-loading.md', 'references/instructions/instructions.md')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Target $required) -PathType Leaf)) {
            return (& $unsafe "required Tooling file is missing: $required")
        }
    }
    if ([IO.File]::ReadAllText((Join-Path $Target 'SKILL.md')) -cnotmatch '(?m)^name: tooling\r?$') {
        return (& $unsafe 'Tooling identity is invalid')
    }
    $version = $null
    $parentState = Join-Path (Split-Path -Parent $Target) 'install-state.json'
    if (Test-Path -LiteralPath $parentState -PathType Leaf) {
        $combined = Read-InstallerCombinedState -Raw ([IO.File]::ReadAllText($parentState)) `
            -WorkTarget (Join-Path (Split-Path -Parent $Target) 'work') -ToolingTarget $Target
        if ($null -ne $combined) { $version = $combined.tooling_version }
    }
    if ($null -eq $version) { return [pscustomobject]@{ Kind = 'first-update'; Reason = ''; Version = $null } }
    return [pscustomobject]@{ Kind = 'managed'; Reason = ''; Version = $version }
}

function Test-InstallerToolingPayload {
    param([string] $Path, [string] $Source, [string] $Target, [string] $Version, [string] $Binary)
    if (-not (Test-InstallerToolingSubset -Path $Path -Source $Source)) { return $false }
    $state = Join-Path $Path 'install-state.json'
    if (-not (Test-Path -LiteralPath $state -PathType Leaf) -or
        (Get-InstallerToolingStateVersion -Raw ([IO.File]::ReadAllText($state)) -Target $Target) -cne $Version) { return $false }
    $installedBinary = Join-Path $Path 'scripts/tooling.exe'
    if (-not (Test-Path -LiteralPath $installedBinary -PathType Leaf) -or
        ((Get-Item -LiteralPath $installedBinary -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        (Get-InstallerWorkFileSha256 $Binary) -cne (Get-InstallerWorkFileSha256 $installedBinary)) { return $false }
    $stack = [System.Collections.Generic.Stack[object]]::new()
    foreach ($entry in @(Get-ChildItem -LiteralPath $Source -Force)) { $stack.Push($entry) }
    while ($stack.Count -gt 0) {
        $entry = $stack.Pop()
        $relative = $entry.FullName.Substring($Source.TrimEnd('\', '/').Length + 1)
        $installed = Join-Path $Path $relative
        if ($entry.PSIsContainer) {
            if (-not (Test-Path -LiteralPath $installed -PathType Container)) { return $false }
            foreach ($child in @(Get-ChildItem -LiteralPath $entry.FullName -Force)) { $stack.Push($child) }
        } else {
            if (-not (Test-Path -LiteralPath $installed -PathType Leaf) -or
                (Get-InstallerWorkFileSha256 $entry.FullName) -cne (Get-InstallerWorkFileSha256 $installed)) { return $false }
        }
    }
    return $true
}
