Set-StrictMode -Version Latest

function New-InstallerCliError {
    param([string] $Message)
    $errorObject = [System.ArgumentException]::new("$Message. Use --help for usage.")
    $errorObject.Data['ExitCode'] = 2
    return $errorObject
}

function Get-InstallerCliUsage {
    @'
Usage: install-agent-work-kit-from-{source,package}.bat [options]
  --host codex|claude
  --skills-dir <parent-directory>
  --instructions-dir <directory>
  --global-instructions yes|no
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
        InstallerHost = $null
        GlobalInstructions = $null
        SkillsDir = $null
        InstructionsDir = $null
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
            { $_ -in @('--host', '--skills-dir', '--instructions-dir', '--global-instructions') } {
                $index++
                if ($index -ge $Tokens.Count -or [string]::IsNullOrEmpty($Tokens[$index]) -or $Tokens[$index].StartsWith('--')) {
                    throw (New-InstallerCliError "Missing value for $flag")
                }
                $value = $Tokens[$index]
                if ($value.Length -ge 2 -and $value.StartsWith('"') -and $value.EndsWith('"')) {
                    $value = $value.Substring(1, $value.Length - 2)
                }
                switch -CaseSensitive ($flag) {
                    '--host' {
                        if ($value -cnotin @('codex', 'claude')) { throw (New-InstallerCliError 'Invalid Host') }
                        $result.InstallerHost = $value
                    }
                    '--global-instructions' {
                        if ($value -cnotin @('yes', 'no')) { throw (New-InstallerCliError 'Invalid Global Instructions choice') }
                        $result.GlobalInstructions = $value -ceq 'yes'
                    }
                    '--skills-dir' { $result.SkillsDir = $value }
                    '--instructions-dir' { $result.InstructionsDir = $value }
                }
            }
        }
    }
    if ($result.GlobalInstructions -eq $false -and $null -ne $result.InstructionsDir) {
        throw (New-InstallerCliError 'Conflicting Global Instructions options')
    }
    if ($result.Action -eq 'install' -and $result.NonInteractive -and $null -eq $result.InstallerHost) {
        throw (New-InstallerCliError 'Missing --host')
    }
    if ($result.Action -ne 'install' -and $result.DryRun) {
        throw (New-InstallerCliError 'Conflicting actions')
    }
    return [pscustomobject] $result
}
