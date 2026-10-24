$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/global-agents/policy.ps1')
$scratch = Join-Path $repoRoot ('.issue95-global.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolved = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolved.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolved) -match '^\.issue95-global\.[0-9a-f]{32}$')) { throw 'unsafe cleanup path' }
try {
    $source = Join-Path $scratch 'source.md'
    $target = Join-Path $scratch 'AGENTS.md'
    [IO.File]::WriteAllText($source, "new`n")
    if ((Get-InstallerGlobalAgentsPlan -Source $source -Target $target -Enabled $false -NonInteractive $true -Confirmed $false).Action -ne 'skip') { throw 'disabled' }
    if ((Get-InstallerGlobalAgentsPlan -Source $source -Target $target -Enabled $true -NonInteractive $true -Confirmed $false).Action -ne 'install') { throw 'missing' }
    [IO.File]::WriteAllText($target, "new`n")
    if ((Get-InstallerGlobalAgentsPlan -Source $source -Target $target -Enabled $true -NonInteractive $true -Confirmed $false).Action -ne 'unchanged') { throw 'identical' }
    [IO.File]::WriteAllText($target, "old`n")
    $plan = Get-InstallerGlobalAgentsPlan -Source $source -Target $target -Enabled $true -NonInteractive $false -Confirmed $false
    if ($plan.Action -ne 'confirm' -or -not $plan.SourceSha256 -or -not $plan.TargetSha256 -or
        ($plan.PSObject.Properties.Name -contains 'Diff')) { throw 'conflict metadata' }
    if ((Get-InstallerGlobalAgentsPlan -Source $source -Target $target -Enabled $true -NonInteractive $false -Confirmed $true).Action -ne 'replace') { throw 'confirmed replacement' }
    try {
        $null = Get-InstallerGlobalAgentsPlan -Source $source -Target $target -Enabled $true -NonInteractive $true -Confirmed $true
        throw 'noninteractive conflict accepted'
    } catch {
        if ($_.Exception.Message -eq 'noninteractive conflict accepted') { throw }
    }
    if ([IO.File]::ReadAllText($target) -cne "old`n") { throw 'policy changed target' }
    Write-Output 'Windows Global AGENTS policy cases passed.'
} finally {
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
