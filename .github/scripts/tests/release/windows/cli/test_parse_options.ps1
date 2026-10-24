$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
$sourceWork = Join-Path $repoRoot 'skills/work'
. (Join-Path $repoRoot 'os-scripts/windows/installer/cli/parse-options.ps1')

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw "FAIL: $Message" }
}

function Assert-Invalid([string[]] $Tokens) {
    try {
        $null = Parse-InstallerCli -Tokens $Tokens -SourceWork $sourceWork
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

Assert-True (@(Get-InstallerCliCatalog | Where-Object { $_.kind -ceq 'skill' -and $_.name -ceq 'spring-boot' -and $_.path -ceq 'programming-language/java/spring-boot' }).Count -eq 1) 'shared Skill catalog'

Assert-True ((Parse-InstallerCli -Tokens @('--help') -SourceWork $sourceWork).Action -eq 'help') 'help action'
Assert-True ((Parse-InstallerCli -Tokens @('-h') -SourceWork $sourceWork).Action -eq 'help') 'short help action'
Assert-True ((Parse-InstallerCli -Tokens @('--list-skills') -SourceWork $sourceWork).Action -eq 'list-skills') 'list action'
Assert-True (@(Get-InstallerAvailableSkills -SourceWork $sourceWork).Contains('spring-boot')) 'actual Skill IDs'
$unicodeDirectory = ([string]::Concat([char]0x76EE, [char]0x9304)) + ' ' +
    ([string]::Concat([char]0x6709, [char]0x7A7A, [char]0x767D)) + ' !&'
$parsed = Parse-InstallerCli -Tokens @('--skills', 'java,spring-boot', '--global-agents', '--work-install-dir', (Join-Path $unicodeDirectory 'work'), '--global-agents-install-dir', $unicodeDirectory, '--non-interactive', '--dry-run', '--verbose') -SourceWork $sourceWork
Assert-True ($parsed.Skills -eq 'java,spring-boot') 'skill value'
Assert-True ($parsed.WorkInstallDir -eq (Join-Path $unicodeDirectory 'work') -and $parsed.GlobalAgentsInstallDir -eq $unicodeDirectory) 'path boundary'
Assert-True ($parsed.NonInteractive -and $parsed.DryRun -and $parsed.Verbose) 'boolean options'
Assert-True ((Parse-InstallerCli -Tokens @('--skills', 'all') -SourceWork $sourceWork).Skills -eq 'all') 'all selection'
Assert-True ((Parse-InstallerCli -Tokens @('--no-global-agents') -SourceWork $sourceWork).GlobalAgents -eq $false) 'global override'

Assert-Invalid @('--help', '-h')
Assert-Invalid @('--skills')
Assert-Invalid @('--skills', '')
Assert-Invalid @('--skills', 'general,general')
Assert-Invalid @('--skills', 'web,')
Assert-Invalid @('--skills', 'web,,general')
Assert-Invalid @('--skills', 'nonexistent')
Assert-Invalid @('--work-install-dir')
Assert-Invalid @('--global-agents-install-dir', '--dry-run')
Assert-Invalid @('--install-dir', 'old/work')
Assert-Invalid @('--global-agents', '--no-global-agents')
Assert-Invalid @('--dry-run', '--list-skills')
Assert-Invalid @('--unknown')
Assert-Invalid @('--yes')
Assert-Invalid @('--HELP')
Assert-Invalid @('--skills', 'JAVA')
Assert-Invalid @('--verbose', '--verbose')

Write-Output 'Windows CLI parser cases passed.'
