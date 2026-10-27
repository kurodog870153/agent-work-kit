Set-StrictMode -Version Latest

function Test-InstallerManagedRelative {
    param([string] $Relative)
    if ($Relative -in @('SKILL.md', 'agents', 'agents/openai.yaml', 'references', 'references/instruction-loading.md', 'references/instruction-loading', 'references/workflows', 'references/subagents', 'references/instructions', 'references/instructions/task', 'references/instructions/execute', 'scripts', 'scripts/work.exe', 'install-state.json')) { return $true }
    return ($Relative -clike 'references/instruction-loading/*' -or
        $Relative -clike 'references/workflows/*' -or
        $Relative -clike 'references/subagents/*' -or
        $Relative -clike 'references/instructions/task/*' -or
        $Relative -clike 'references/instructions/execute/*')
}

function Test-InstallerLegacyWorkIdentity {
    param([string] $Target)
    $binary = Join-Path $Target 'scripts/work.exe'
    try {
        $stream = [IO.File]::OpenRead($binary)
        try {
            $reader = [IO.BinaryReader]::new($stream)
            if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) { return $false }
            $stream.Position = 0x3C
            $peOffset = $reader.ReadInt32()
            if ($peOffset -lt 64 -or $peOffset -gt $stream.Length - 4) { return $false }
            $stream.Position = $peOffset
            if ($reader.ReadUInt32() -ne 0x00004550) { return $false }
        } finally { $stream.Dispose() }
        $response = & $binary --help 2>$null | Out-String | ConvertFrom-Json -ErrorAction Stop
        if ($LASTEXITCODE -ne 0 -or $response.schema -cne 'work-cli-result' -or
            $response.status -cne 'success' -or $response.data.help -cnotmatch '(?i)^usage: work\b') { return $false }
        $markers = @{
            'SKILL.md' = '(?m)^name: work\r?$'
            'agents/openai.yaml' = '(?m)^  display_name: "Work"\r?$'
            'references/instruction-loading.md' = '(?m)^# Work Instruction Loading Bootstrap\r?$'
            'references/instruction-loading/invocation.md' = '(?m)^# Invocation contract\r?$'
            'references/instructions/task/general/instructions.md' = '(?m)^# (任務規劃指令|Task planning)\r?$'
            'references/instructions/execute/general/instructions.md' = '(?m)^# (任務執行指令|Task execution)\r?$'
        }
        foreach ($relative in $markers.Keys) {
            $content = Get-Content -LiteralPath (Join-Path $Target $relative) -Raw -ErrorAction Stop
            if ($content -cnotmatch $markers[$relative]) { return $false }
        }
        return $true
    } catch { return $false }
}

function Inspect-InstallerTarget {
    param([string] $Target, [string] $SourceWork, [bool] $StateValid)
    $unsafe = { param($Reason) [pscustomobject]@{ Kind = 'unsafe'; Reason = $Reason; RequiresConfirmation = $false } }
    if (-not [IO.Path]::IsPathRooted($Target)) { return (& $unsafe 'target is not an absolute Work path') }
    $probe = $Target
    while (-not [string]::IsNullOrEmpty($probe)) {
        if (Test-Path -LiteralPath $probe) {
            $item = Get-Item -LiteralPath $probe -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                return (& $unsafe "symbolic link in target path: $probe")
            }
        }
        $parent = Split-Path -Path $probe -Parent
        if ($parent -eq $probe) { break }
        $probe = $parent
    }
    if (-not (Test-Path -LiteralPath $Target)) {
        return [pscustomobject]@{ Kind = 'new'; Reason = ''; RequiresConfirmation = $false }
    }
    if (-not (Test-Path -LiteralPath $Target -PathType Container)) { return (& $unsafe 'target is not a directory') }
    $children = @(Get-ChildItem -LiteralPath $Target -Force)
    if ($children.Count -eq 0) { return [pscustomobject]@{ Kind = 'new'; Reason = ''; RequiresConfirmation = $false } }
    $stack = [System.Collections.Generic.Stack[object]]::new()
    foreach ($child in $children) { $stack.Push($child) }
    while ($stack.Count -gt 0) {
        $entry = $stack.Pop()
        $relative = $entry.FullName.Substring($Target.TrimEnd('\', '/').Length + 1).Replace('\', '/')
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return (& $unsafe "unsafe entry: $relative") }
        if (-not (Test-InstallerManagedRelative $relative)) { return (& $unsafe "unknown entry: $relative") }
        if ($relative -notin @('scripts', 'scripts/work.exe', 'install-state.json') -and
            -not (Test-Path -LiteralPath (Join-Path $SourceWork $relative))) {
            return (& $unsafe "unrecognized Work entry: $relative")
        }
        if ($entry.PSIsContainer) {
            foreach ($child in @(Get-ChildItem -LiteralPath $entry.FullName -Force)) { $stack.Push($child) }
        }
    }
    foreach ($relative in @('SKILL.md', 'agents/openai.yaml', 'references/instruction-loading.md', 'references/instruction-loading/invocation.md', 'references/instructions/task/general/instructions.md', 'references/instructions/execute/general/instructions.md', 'scripts/work.exe')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Target $relative) -PathType Leaf)) {
            return (& $unsafe "required Work file is missing: $relative")
        }
    }
    if (-not $StateValid -and -not (Test-InstallerLegacyWorkIdentity -Target $Target)) {
        return (& $unsafe 'existing Work binary or managed content is not recognizable')
    }
    if ($StateValid) { return [pscustomobject]@{ Kind = 'managed'; Reason = ''; RequiresConfirmation = $false } }
    return [pscustomobject]@{ Kind = 'first-update'; Reason = ''; RequiresConfirmation = $true }
}

function Test-InstallerTargetAuthorization {
    param([object] $Inspection, [bool] $NonInteractive, [bool] $Confirmed)
    if ($Inspection.Kind -in @('new', 'managed')) { return $true }
    if ($Inspection.Kind -eq 'first-update') { return (-not $NonInteractive -and $Confirmed) }
    return $false
}
