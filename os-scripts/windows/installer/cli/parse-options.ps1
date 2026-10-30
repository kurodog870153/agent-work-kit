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
  --global-agent | --no-global-agent
  --skills-install-dir <parent-directory>
  --global-agent-install-dir <directory>
  --non-interactive
  --dry-run
  --verbose
  --help, -h
'@
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

function Parse-InstallerCli {
    param(
        [string[]] $Tokens = @()
    )
    $result = [ordered]@{
        Action = 'install'
        GlobalAgents = $null
        SkillsInstallDir = $null
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
            '--non-interactive' { $result.NonInteractive = $true }
            '--dry-run' { $result.DryRun = $true }
            '--verbose' { $result.Verbose = $true }
            '--global-agent' { $result.GlobalAgents = $true }
            '--no-global-agent' { $result.GlobalAgents = $false }
            { $_ -in @('--skills-install-dir', '--global-agent-install-dir') } {
                $index++
                if ($index -ge $Tokens.Count -or [string]::IsNullOrEmpty($Tokens[$index]) -or $Tokens[$index].StartsWith('--')) {
                    throw (New-InstallerCliError "Missing value for $flag")
                }
                $value = $Tokens[$index]
                if ($value.Length -ge 2 -and $value.StartsWith('"') -and $value.EndsWith('"')) {
                    $value = $value.Substring(1, $value.Length - 2)
                }
                if ($flag -ceq '--skills-install-dir') { $result.SkillsInstallDir = $value }
                else { $result.GlobalAgentsInstallDir = $value }
            }
        }
    }
    if ($seen.Contains('--global-agent') -and $seen.Contains('--no-global-agent')) {
        throw (New-InstallerCliError 'Conflicting Global AGENTS options')
    }
    if ($result.Action -ne 'install' -and $result.DryRun) {
        throw (New-InstallerCliError 'Conflicting actions')
    }
    return [pscustomobject] $result
}
