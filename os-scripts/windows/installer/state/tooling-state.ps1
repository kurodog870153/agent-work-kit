Set-StrictMode -Version Latest

function Get-InstallerToolingStateVersion {
    param([string] $Raw, [string] $Target)
    try { $value = ConvertFrom-Json -InputObject $Raw -ErrorAction Stop } catch { return $null }
    if ($value -isnot [pscustomobject] -or
        (@($value.PSObject.Properties.Name | Sort-Object -CaseSensitive) -join ',') -cne 'skill,tooling_dir,version' -or
        $value.skill -cne 'tooling' -or
        -not [string]::Equals($value.tooling_dir, $Target, [StringComparison]::OrdinalIgnoreCase) -or
        $value.version -isnot [string] -or $value.version -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
        return $null
    }
    return $value.version
}

