$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/../../../release/windows/firewall_isolation.ps1"

function New-NetFirewallRule {
    param($Name, $DisplayName, $Direction, $Action, $Profile, $Enabled, $ErrorAction)
    return $null
}
function Get-NetFirewallRule {
    param($PolicyStore, $Name, $ErrorAction)
    if ($PolicyStore -eq 'ActiveStore') { return $null }
    return [pscustomobject]@{ Enabled = 'True'; Action = 'Block' }
}
function Remove-NetFirewallRule {
    param($Name, $ErrorAction)
    if ($script:failRemoval) { throw 'simulated firewall removal failure' }
}
function Clear-DnsClientCache { }
function Start-Sleep { }
function Write-Host { param([string] $Object) $script:messages.Add($Object) }

$script:messages = [System.Collections.Generic.List[string]]::new()
$script:failRemoval = $true
try {
    Invoke-ReleaseE2EIsolation -Rule 'test-rule' -Verify { throw 'simulated E2E failure' }
    throw 'expected E2E failure was not raised'
} catch {
    if ($_.Exception.Message -notmatch 'simulated E2E failure') { throw }
}
if (($script:messages -join "`n") -notmatch 'remove isolation rule:.*simulated firewall removal failure') {
    throw 'firewall cleanup failure was not logged alongside the E2E failure'
}

$script:messages.Clear()
try {
    Invoke-ReleaseE2EIsolation -Rule 'test-rule' -Verify { }
    throw 'expected cleanup failure was not raised'
} catch {
    if ($_.Exception.Message -notmatch 'simulated firewall removal failure') { throw }
}

$script:failRemoval = $false
$script:messages.Clear()
try {
    Invoke-ReleaseE2EIsolation -Rule 'test-rule' -Verify { throw 'simulated E2E failure' }
    throw 'expected E2E failure was not raised'
} catch {
    if ($_.Exception.Message -notmatch 'simulated E2E failure') { throw }
}
if (($script:messages -join "`n") -notmatch 'network isolation removed') {
    throw 'successful firewall cleanup was not logged'
}
Write-Output 'Windows firewall isolation failure tests passed.'
