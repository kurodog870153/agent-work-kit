$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/cli/parse-options.ps1')

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw "FAIL: $Message" }
}

function Assert-Invalid([string[]] $Tokens) {
    try {
        $null = Parse-InstallerCli -Tokens $Tokens
    } catch {
        $errorCode = $_.Exception.Data['ExitCode']
        if ($null -eq $errorCode -and $null -ne $_.Exception.InnerException) {
            $errorCode = $_.Exception.InnerException.Data['ExitCode']
        }
        Assert-True ($errorCode -eq 2) "Wrong error code for $($Tokens -join ' ')"
        return
    }
    throw "FAIL: accepted invalid arguments: $($Tokens -join ' ')"
}

Assert-True (@(Get-InstallerCliCatalog | Where-Object { $_.kind -ceq 'skill' }).Count -eq 0) 'option-only catalog'

Assert-True ((Parse-InstallerCli -Tokens @('--help')).Action -eq 'help') 'help action'
Assert-True ((Parse-InstallerCli -Tokens @('-h')).Action -eq 'help') 'short help action'
$unicodeDirectory = ([string]::Concat([char]0x76EE, [char]0x9304)) + ' ' +
    ([string]::Concat([char]0x6709, [char]0x7A7A, [char]0x767D)) + ' !&'
$parsed = Parse-InstallerCli -Tokens @('--global-agent', '--skills-install-dir', $unicodeDirectory, '--global-agent-install-dir', $unicodeDirectory, '--non-interactive', '--dry-run', '--verbose')
Assert-True ($parsed.SkillsInstallDir -eq $unicodeDirectory -and $parsed.GlobalAgentsInstallDir -eq $unicodeDirectory) 'path boundary'
Assert-True ($parsed.NonInteractive -and $parsed.DryRun -and $parsed.Verbose) 'boolean options'
Assert-True ((Parse-InstallerCli -Tokens @('--no-global-agent')).GlobalAgents -eq $false) 'global override'

Assert-Invalid @('--help', '-h')
Assert-Invalid @('--skills')
Assert-Invalid @('--skills', 'general')
Assert-Invalid @('--list-skills')
Assert-Invalid @('--work-install-dir')
Assert-Invalid @('--global-agents')
Assert-Invalid @('--no-global-agents')
Assert-Invalid @('--skill-install-dir', 'old/work')
Assert-Invalid @('--global-agents-install-dir', 'old/global')
Assert-Invalid @('--skills-install-dir')
Assert-Invalid @('--skills-install-dir', '')
Assert-Invalid @('--global-agent-install-dir', '--dry-run')
Assert-Invalid @('--install-dir', 'old/work')
Assert-Invalid @('--global-agent', '--no-global-agent')
Assert-Invalid @('--unknown')
Assert-Invalid @('--yes')
Assert-Invalid @('--HELP')
Assert-Invalid @('--verbose', '--verbose')

Write-Output 'Windows CLI parser cases passed.'
