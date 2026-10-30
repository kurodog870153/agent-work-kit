Set-StrictMode -Version Latest

function Get-InstallerExpandedSkills {
    param([Parameter(Mandatory = $true)][string] $Selection)
    $order = @('general', 'web', 'backend', 'frontend', 'astro', 'css', 'tailwind', 'programming-language', 'java', 'spring-boot', 'persistence', 'jpa', 'mybatis', 'typescript')
    $ancestors = @{
        'general' = @()
        'web' = @('general')
        'backend' = @('general', 'web')
        'frontend' = @('general', 'web')
        'astro' = @('general', 'web', 'frontend')
        'css' = @('general', 'web', 'frontend')
        'tailwind' = @('general', 'web', 'frontend', 'css')
        'programming-language' = @('general')
        'java' = @('general', 'programming-language')
        'spring-boot' = @('general', 'programming-language', 'java')
        'persistence' = @('general', 'programming-language', 'java')
        'jpa' = @('general', 'programming-language', 'java', 'persistence')
        'mybatis' = @('general', 'programming-language', 'java', 'persistence')
        'typescript' = @('general', 'programming-language')
    }
    $selected = if ($Selection -ceq 'all') { $order } else { $Selection.Split(',') }
    $seenInput = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $included = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $null = $included.Add('general')
    foreach ($id in $selected) {
        if (-not $ancestors.ContainsKey($id) -or -not $seenInput.Add($id)) { throw 'invalid Skill ID' }
        foreach ($parent in $ancestors[$id]) { $null = $included.Add($parent) }
        $null = $included.Add($id)
    }
    foreach ($id in $order) { if ($included.Contains($id)) { $id } }
}

function Test-InstallerStateVersion {
    param($Version)
    return ($Version -is [string] -and $Version -cmatch '^[0-9]+\.[0-9]+\.[0-9]+$')
}

function New-InstallerState {
    param(
        [string] $TemplateRaw,
        [string] $Version,
        [string] $Skills,
        [bool] $GlobalAgents,
        [string] $WorkDir,
        [AllowNull()][string] $GlobalAgentsDir
    )
    try { $template = ConvertFrom-Json -InputObject $TemplateRaw -ErrorAction Stop }
    catch { throw 'invalid install-state template' }
    if ($template -isnot [pscustomobject]) { throw 'invalid install-state template' }
    $keys = @($template.PSObject.Properties.Name | Sort-Object -CaseSensitive) -join ','
    if ($keys -cne 'global_agent_dir,skill_install_dir,status,tooling_version,version,work_version' -or
        @($template.PSObject.Properties | Where-Object { $null -ne $_.Value }).Count -ne 0 -or
        -not (Test-InstallerStateVersion $Version) -or
        [string]::IsNullOrEmpty($WorkDir) -or -not [IO.Path]::IsPathRooted($WorkDir) -or
        ($GlobalAgents -and ([string]::IsNullOrEmpty($GlobalAgentsDir) -or
            -not [IO.Path]::IsPathRooted($GlobalAgentsDir)))) {
        throw 'invalid state input'
    }
    $resolved = @(Get-InstallerExpandedSkills -Selection $Skills)
    return [ordered]@{
        version = $Version
        skills = $resolved
        work_dir = $WorkDir
        global_agents_dir = $(if ($GlobalAgents) { $GlobalAgentsDir } else { $null })
    }
}

function Read-InstallerState {
    param([string] $Raw, [string] $InstallDir)
    try { $value = ConvertFrom-Json -InputObject $Raw -ErrorAction Stop } catch { return $null }
    if ($null -eq $value -or $value -isnot [pscustomobject]) { return $null }
    $keys = @($value.PSObject.Properties.Name | Sort-Object -CaseSensitive) -join ','
    if ($keys -cne 'global_agents_dir,skills,version,work_dir' -or
        -not (Test-InstallerStateVersion $value.version) -or
        $value.work_dir -isnot [string] -or
        -not [string]::Equals($value.work_dir, $InstallDir, [StringComparison]::OrdinalIgnoreCase) -or
        ($null -ne $value.global_agents_dir -and
            ($value.global_agents_dir -isnot [string] -or
                -not [IO.Path]::IsPathRooted($value.global_agents_dir))) -or
        $value.skills -isnot [array] -or $value.skills.Count -eq 0) { return $null }
    foreach ($id in $value.skills) { if ($id -isnot [string]) { return $null } }
    try { $expanded = @(Get-InstallerExpandedSkills -Selection ($value.skills -join ',')) } catch { return $null }
    if (($expanded -join ',') -cne ($value.skills -join ',')) { return $null }
    return $value
}

function Resolve-InstallerSettings {
    param(
        [string] $Raw,
        [string] $InstallDir,
        [string] $SkillsOverride,
        [bool] $GlobalAgentsOverride
    )
    $state = Read-InstallerState -Raw $Raw -InstallDir $InstallDir
    $skills = if ($PSBoundParameters.ContainsKey('SkillsOverride')) { @(Get-InstallerExpandedSkills -Selection $SkillsOverride) } elseif ($null -ne $state) { @($state.skills) } else { @('general') }
    $globalAgents = if ($PSBoundParameters.ContainsKey('GlobalAgentsOverride')) { $GlobalAgentsOverride } elseif ($null -ne $state) { $null -ne $state.global_agents_dir } else { $true }
    return [pscustomobject]@{
        HasValidState = ($null -ne $state)
        Skills = $skills
        GlobalAgents = $globalAgents
        GlobalAgentsDir = $(if ($null -ne $state) { $state.global_agents_dir } else { $null })
    }
}
