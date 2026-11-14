Set-StrictMode -Version Latest

function Test-InstallerContractSubset {
    param([string] $Path, [string] $Source)
    if (-not (Test-Path -LiteralPath $Path -PathType Container) -or
        ((Get-Item -LiteralPath $Path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    $stack = [System.Collections.Generic.Stack[object]]::new()
    foreach ($entry in @(Get-ChildItem -LiteralPath $Path -Force)) { $stack.Push($entry) }
    while ($stack.Count -gt 0) {
        $entry = $stack.Pop()
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
        $relative = $entry.FullName.Substring($Path.TrimEnd('\', '/').Length + 1)
        $sourcePath = Join-Path $Source $relative
        if ($entry.PSIsContainer) {
            if (-not (Test-Path -LiteralPath $sourcePath -PathType Container)) { return $false }
            foreach ($child in @(Get-ChildItem -LiteralPath $entry.FullName -Force)) { $stack.Push($child) }
        } elseif (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf) -or
            $entry.Extension -cnotin @('.md', '.yaml', '.yml')) { return $false }
    }
    return $true
}

function Test-InstallerContractSource {
    param([string] $Source)
    if (-not (Test-InstallerContractSubset -Path $Source -Source $Source)) { return $false }
    foreach ($required in @('SKILL.md', 'references/instruction-loading.md', 'references/instruction-loading/queries.md')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Source $required) -PathType Leaf)) { return $false }
    }
    return ([IO.File]::ReadAllText((Join-Path $Source 'SKILL.md')) -cmatch '(?m)^name: contract\r?$')
}

function Test-InstallerContractPayload {
    param([string] $Path, [string] $Source)
    if (-not (Test-InstallerContractSource -Source $Source) -or
        -not (Test-InstallerContractSubset -Path $Path -Source $Source)) { return $false }
    $stack = [System.Collections.Generic.Stack[object]]::new()
    foreach ($entry in @(Get-ChildItem -LiteralPath $Source -Force)) { $stack.Push($entry) }
    while ($stack.Count -gt 0) {
        $entry = $stack.Pop()
        $relative = $entry.FullName.Substring($Source.TrimEnd('\', '/').Length + 1)
        $installed = Join-Path $Path $relative
        if ($entry.PSIsContainer) {
            if (-not (Test-Path -LiteralPath $installed -PathType Container)) { return $false }
            foreach ($child in @(Get-ChildItem -LiteralPath $entry.FullName -Force)) { $stack.Push($child) }
        } elseif (-not (Test-Path -LiteralPath $installed -PathType Leaf) -or
            (Get-InstallerWorkFileSha256 $entry.FullName) -cne (Get-InstallerWorkFileSha256 $installed)) { return $false }
    }
    return $true
}

function Inspect-InstallerContractTarget {
    param([string] $Target, [string] $Source, [string] $InstallerHost)
    $unsafe = { param($Reason) [pscustomobject]@{ Kind = 'unsafe'; Reason = $Reason } }
    if (-not [IO.Path]::IsPathRooted($Target) -or [IO.Path]::GetFileName($Target) -cne 'contract' -or
        -not (Test-InstallerDirectoryPathSafe -Path $Target)) { return (& $unsafe 'unsafe Contract target path') }
    if (-not (Test-Path -LiteralPath $Target)) { return [pscustomobject]@{ Kind = 'new'; Reason = '' } }
    if (-not (Test-Path -LiteralPath $Target -PathType Container)) { return (& $unsafe 'Contract target is not a directory') }
    if (@(Get-ChildItem -LiteralPath $Target -Force).Count -eq 0) { return [pscustomobject]@{ Kind = 'new'; Reason = '' } }
    if (-not (Test-InstallerContractSubset -Path $Target -Source $Source)) { return (& $unsafe 'unknown or unsafe Contract entry') }
    if (-not (Test-Path -LiteralPath (Join-Path $Target 'SKILL.md') -PathType Leaf) -or
        [IO.File]::ReadAllText((Join-Path $Target 'SKILL.md')) -cnotmatch '(?m)^name: contract\r?$') {
        return (& $unsafe 'Contract identity is invalid')
    }
    $parent = Split-Path -Parent $Target
    $statePath = Join-Path $parent 'install-state.json'
    if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) { return (& $unsafe 'Contract has no parent install state') }
    $state = Read-InstallerCombinedState -Raw ([IO.File]::ReadAllText($statePath)) `
        -WorkTarget (Join-Path $parent 'work') -ToolingTarget (Join-Path $parent 'tooling') -InstallerHost $InstallerHost
    if ($null -eq $state) { return (& $unsafe 'Contract parent install state is invalid') }
    return [pscustomobject]@{ Kind = 'managed'; Reason = '' }
}
