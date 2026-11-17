$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/../../../release/windows/firewall_isolation.ps1"

# Mock the Windows firewall provider to verify scope and cleanup without
# touching the runner's real firewall during unit tests.
$global:rules = @{}
$global:messages = [System.Collections.Generic.List[string]]::new()
$global:failCreation = $false
$global:failRemoval = $false
$global:createdPrograms = [System.Collections.Generic.List[string]]::new()

function New-NetFirewallRule {
    param($Name, $DisplayName, $Program, $Direction, $Action, $Profile, $Enabled, $ErrorAction)
    if (-not $Program -or -not [IO.Path]::IsPathRooted($Program)) {
        throw 'Unscoped outbound firewall rule was requested'
    }
    if ($global:failCreation) { throw 'simulated rule creation failure' }
    if ($Direction -ne 'Outbound' -or $Action -ne 'Block') { throw 'Invalid firewall action' }
    $global:createdPrograms.Add($Program)
    $global:rules[$Name] = [pscustomobject]@{
        Name = $Name
        Program = $Program
        Enabled = 'True'
        Action = 'Block'
    }
    return $global:rules[$Name]
}
function Get-NetFirewallRule {
    param($Name, $PolicyStore, $ErrorAction)
    foreach ($key in @($global:rules.Keys | Where-Object { $_ -like $Name })) {
        $global:rules[$key]
    }
}
function Get-NetFirewallApplicationFilter {
    param($AssociatedNetFirewallRule, $ErrorAction)
    return [pscustomobject]@{ Program = $AssociatedNetFirewallRule.Program }
}
function Remove-NetFirewallRule {
    param($Name, $ErrorAction)
    if ($global:failRemoval) { throw 'simulated firewall removal failure' }
    $global:rules.Remove($Name) | Out-Null
}
function Write-Host { param([string] $Object) $global:messages.Add($Object) }

$programs = @('C:\Windows\System32\cmd.exe', 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe')
Invoke-ReleaseE2EIsolation -Rule 'test-rule' -Programs $programs -Verify {
    if ($global:rules.Count -ne 2) { throw 'Expected two scoped firewall rules' }
    if (@($global:rules.Values | Where-Object { -not $_.Program }).Count -ne 0) {
        throw 'An E2E rule was not restricted to a program'
    }
    # Dynamically registered installed executables must share the same scope
    # and be removed by the outer owner.
    Add-ReleaseE2EProgramRules -Rule 'test-rule' -Programs @('C:\fixture\work.exe')
    if ($global:rules.Count -ne 3) { throw 'Dynamic CLI program was not restricted' }
    Add-ReleaseE2EProgramRules -Rule 'test-rule' -Programs @('C:\fixture\work.exe')
    if ($global:rules.Count -ne 3) { throw 'Duplicate program created a second rule' }
}
if ($global:rules.Count) { throw 'Successful isolation left firewall rules behind' }
if ($global:createdPrograms.Count -ne 3) { throw 'E2E program restrictions were not created correctly' }

try {
    Add-ReleaseE2EProgramRules -Rule 'test-rule' -Programs @('C:\Program Files\PowerShell\7\pwsh.exe')
    throw 'Expected refusing to isolate the GitHub Actions supervisor'
} catch {
    if ($_.Exception.Message -notmatch 'Refusing to block') { throw }
}
try {
    Add-ReleaseE2EProgramRules -Rule 'test-rule' -Programs @('cmd.exe')
    throw 'Expected absolute executable path validation'
} catch {
    if ($_.Exception.Message -notmatch 'absolute path') { throw }
}

# Reproduce the Python-to-Windows-PowerShell JSON array contract.
# The registration script must consume two strings, not a nested Object[].
$pathsFile = Join-Path ([IO.Path]::GetTempPath()) ("e2e-cli-paths-" + [guid]::NewGuid() + ".json")
try {
    $fixturePaths = @('C:\fixture\test-work.exe', 'C:\fixture\test-skill.exe')
    [IO.File]::WriteAllText($pathsFile, (ConvertTo-Json -InputObject $fixturePaths -Compress))
    & "$PSScriptRoot/../../../release/windows/register_programs.ps1" -Rule 'test-rule' -PathsFile $pathsFile
    if ($global:rules.Count -ne 2) { throw 'JSON CLI path registration did not create two scoped rules' }
    Remove-ReleaseE2EProgramRules -Rule 'test-rule'

    [IO.File]::WriteAllText($pathsFile, '{"paths":["C:\\fixture\\test-work.exe"]}')
    try {
        & "$PSScriptRoot/../../../release/windows/register_programs.ps1" -Rule 'test-rule' -PathsFile $pathsFile
        throw 'Expected invalid JSON structure to be rejected'
    } catch {
        if ($_.Exception.Message -notmatch 'Invalid CLI program list') { throw }
    }
    if ($global:rules.Count) { throw 'Invalid CLI program list created firewall rules' }
} finally {
    Remove-Item -LiteralPath $pathsFile -ErrorAction SilentlyContinue
}

$global:failCreation = $true
try {
    Invoke-ReleaseE2EIsolation -Rule 'test-rule' -Programs $programs -Verify { }
    throw 'Expected setup failure'
} catch {
    if ($_.Exception.Message -notmatch 'simulated rule creation failure') { throw }
}
if ($global:rules.Count) { throw 'Partial firewall setup was not cleaned up' }

$global:failCreation = $false
$global:failRemoval = $true
try {
    Invoke-ReleaseE2EIsolation -Rule 'test-rule' -Programs $programs -Verify { throw 'simulated E2E failure' }
    throw 'Expected original E2E failure'
} catch {
    if ($_.Exception.Message -notmatch 'simulated E2E failure') { throw }
}
if (($global:messages -join "`n") -notmatch 'cleanup incomplete') {
    throw 'Cleanup failure was not logged alongside E2E failure'
}
$global:failRemoval = $false
Remove-ReleaseE2EProgramRules -Rule 'test-rule'
if ($global:rules.Count) { throw 'Firewall test cleanup left rules behind' }

$global:failRemoval = $true
try {
    Invoke-ReleaseE2EIsolation -Rule 'test-rule' -Programs $programs -Verify { }
    throw 'Expected cleanup error'
} catch {
    if ($_.Exception.Message -notmatch 'simulated firewall removal failure') { throw }
}
$global:failRemoval = $false
Remove-ReleaseE2EProgramRules -Rule 'test-rule'
if ($global:rules.Count) { throw 'Firewall mock teardown failed' }

$global:messages.Clear()
try {
    Invoke-ReleaseE2EIsolation -Rule 'test-rule' -Programs $programs -Verify { throw 'simulated E2E failure' }
    throw 'Expected E2E failure'
} catch {
    if ($_.Exception.Message -notmatch 'simulated E2E failure') { throw }
}
if (($global:messages -join "`n") -notmatch 'isolation removed') {
    throw 'Successful firewall cleanup was not logged'
}
if ($global:rules.Count) { throw 'Failure-path firewall rules remain' }
Write-Output 'Windows scoped firewall isolation tests passed.'
