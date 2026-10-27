Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot 'work-state.ps1')
. (Join-Path $PSScriptRoot 'tooling-state.ps1')

function Read-InstallerCombinedState {
    param([string] $Raw, [string] $WorkTarget, [string] $ToolingTarget)
    try { $value = ConvertFrom-Json -InputObject $Raw -ErrorAction Stop } catch { return $null }
    $parent = Split-Path -Parent $WorkTarget
    if ($value -isnot [pscustomobject] -or
        (@($value.PSObject.Properties.Name | Sort-Object -CaseSensitive) -join ',') -cne 'global_agent_dir,skill_install_dir,status,tooling_version,version,work_version' -or
        $value.status -cnotin @('installing', 'installed') -or
        -not (Test-InstallerStateVersion $value.version) -or
        -not (Test-InstallerStateVersion $value.work_version) -or
        -not (Test-InstallerStateVersion $value.tooling_version) -or
        $value.skill_install_dir -isnot [string] -or
        -not [string]::Equals($value.skill_install_dir, $parent, [StringComparison]::OrdinalIgnoreCase) -or
        ($null -ne $value.global_agent_dir -and
            ($value.global_agent_dir -isnot [string] -or -not [IO.Path]::IsPathRooted($value.global_agent_dir)))) {
        return $null
    }
    return $value
}

function Write-InstallerCombinedState {
    param([string] $Parent, [string] $Version, [string] $WorkVersion, [string] $ToolingVersion,
        [AllowNull()][string] $GlobalAgentDir, [string] $Status)
    $target = Join-Path $Parent 'install-state.json'
    $temporary = Join-Path $Parent '.install-state.json.tmp'
    $backup = Join-Path $Parent '.install-state.json.previous'
    if (Test-Path -LiteralPath $temporary) { throw "pending state write needs review: $temporary" }
    if (Test-Path -LiteralPath $backup) { throw "previous state write needs review: $backup" }
    if ((Test-Path -LiteralPath $target) -and
        ((Get-Item -LiteralPath $target -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'unsafe parent install-state.json'
    }
    $value = [ordered]@{
        version = $Version; work_version = $WorkVersion; tooling_version = $ToolingVersion
        skill_install_dir = $Parent
        global_agent_dir = $(if ([string]::IsNullOrEmpty($GlobalAgentDir)) { $null } else { $GlobalAgentDir })
        status = $Status
    }
    $json = ($value | ConvertTo-Json -Depth 6) + "`n"
    [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
    if (Test-Path -LiteralPath $target) {
        [IO.File]::Replace($temporary, $target, $backup)
        Remove-Item -LiteralPath $backup -ErrorAction Stop
    } else {
        Move-Item -LiteralPath $temporary -Destination $target -ErrorAction Stop
    }
}

