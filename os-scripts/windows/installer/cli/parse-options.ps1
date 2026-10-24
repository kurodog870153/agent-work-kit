Set-StrictMode -Version Latest

function New-InstallerCliError {
    param([string] $Message)
    $errorObject = [System.ArgumentException]::new("$Message. Use --help for usage.")
    $errorObject.Data['ExitCode'] = 2
    return $errorObject
}

function Get-InstallerCliUsage {
    @'
Usage: install-work-from-{source,package}.bat [options]
  --skills <general|all|id[,id...]>
  --global-agents | --no-global-agents
  --work-install-dir <work-directory>
  --global-agents-install-dir <directory>
  --non-interactive
  --dry-run
  --verbose
  --list-skills
  --help, -h
'@
}

function Get-InstallerSkillPaths {
    $paths = [ordered]@{}
    foreach ($entry in Get-InstallerCliCatalog) {
        if ($entry.kind -ceq 'skill') {
            $paths[$entry.name] = $entry.path
        }
    }
    return $paths
}

function Get-InstallerCliCatalog {
    $catalog = Join-Path $PSScriptRoot 'options.tsv'
    if (-not (Test-Path -LiteralPath $catalog -PathType Leaf)) {
        $catalog = Join-Path $PSScriptRoot '../../../common/installer/options.tsv'
    }
    if (-not (Test-Path -LiteralPath $catalog -PathType Leaf)) {
        throw 'Installer option catalog is missing'
    }
    return @(Import-Csv -LiteralPath $catalog -Delimiter "`t" -Encoding UTF8)
}

function Get-InstallerAvailableSkills {
    param([Parameter(Mandatory = $true)][string] $SourceWork)
    $paths = Get-InstallerSkillPaths
    foreach ($id in $paths.Keys) {
        $relative = $paths[$id]
        $task = Join-Path $SourceWork "references/instructions/task/$relative/instructions.md"
        $execute = Join-Path $SourceWork "references/instructions/execute/$relative/instructions.md"
        if ((Test-Path -LiteralPath $task -PathType Leaf) -and (Test-Path -LiteralPath $execute -PathType Leaf)) {
            $id
        }
    }
}

function Test-InstallerSkillSelection {
    param([string] $Value, [string] $SourceWork)
    if ([string]::IsNullOrEmpty($Value)) { return $false }
    if ($Value -ceq 'general' -or $Value -ceq 'all') { return $true }
    $available = @(Get-InstallerAvailableSkills -SourceWork $SourceWork)
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($id in $Value.Split(',')) {
        if (-not $available.Contains($id) -or -not $seen.Add($id)) { return $false }
    }
    return $true
}

function Parse-InstallerCli {
    param(
        [string[]] $Tokens = @(),
        [Parameter(Mandatory = $true)][string] $SourceWork
    )
    $result = [ordered]@{
        Action = 'install'
        Skills = $null
        GlobalAgents = $null
        WorkInstallDir = $null
        GlobalAgentsInstallDir = $null
        NonInteractive = $false
        DryRun = $false
        Verbose = $false
    }
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $allowed = @(Get-InstallerCliCatalog | Where-Object { $_.kind -ceq 'option' } | ForEach-Object { $_.name })
    for ($index = 0; $index -lt $Tokens.Count; $index++) {
        $flag = $Tokens[$index]
        if ($flag -cnotin $allowed) {
            throw (New-InstallerCliError "Unknown option $flag")
        }
        if ($flag -ceq '-h') { $flag = '--help' }
        if (-not $seen.Add($flag)) { throw (New-InstallerCliError "Duplicate option $flag") }
        switch -CaseSensitive ($flag) {
            '--help' { $result.Action = 'help' }
            '--list-skills' { $result.Action = 'list-skills' }
            '--non-interactive' { $result.NonInteractive = $true }
            '--dry-run' { $result.DryRun = $true }
            '--verbose' { $result.Verbose = $true }
            '--global-agents' { $result.GlobalAgents = $true }
            '--no-global-agents' { $result.GlobalAgents = $false }
            { $_ -in @('--skills', '--work-install-dir', '--global-agents-install-dir') } {
                $index++
                if ($index -ge $Tokens.Count -or [string]::IsNullOrEmpty($Tokens[$index]) -or $Tokens[$index].StartsWith('--')) {
                    throw (New-InstallerCliError "Missing value for $flag")
                }
                $value = $Tokens[$index]
                if ($flag -ceq '--skills') {
                    if (-not (Test-InstallerSkillSelection -Value $value -SourceWork $SourceWork)) {
                        throw (New-InstallerCliError "Invalid Skill selection $value")
                    }
                    $result.Skills = $value
                } else {
                    if ($value.Length -ge 2 -and $value.StartsWith('"') -and $value.EndsWith('"')) {
                        $value = $value.Substring(1, $value.Length - 2)
                    }
                    if ($flag -ceq '--work-install-dir') { $result.WorkInstallDir = $value }
                    else { $result.GlobalAgentsInstallDir = $value }
                }
            }
        }
    }
    if ($seen.Contains('--global-agents') -and $seen.Contains('--no-global-agents')) {
        throw (New-InstallerCliError 'Conflicting Global AGENTS options')
    }
    if (($seen.Contains('--help') -and $seen.Contains('--list-skills')) -or ($result.Action -ne 'install' -and $result.DryRun)) {
        throw (New-InstallerCliError 'Conflicting actions')
    }
    return [pscustomobject] $result
}
