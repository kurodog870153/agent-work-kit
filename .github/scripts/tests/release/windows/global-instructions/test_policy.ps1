$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/state/state.ps1')
. (Join-Path $repoRoot 'os-scripts/windows/installer/global-instructions/policy.ps1')
$scratch = Join-Path $repoRoot ('.installer-test-global.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolved = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolved.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolved) -match '^\.installer-test-global\.[0-9a-f]{32}$')) { throw 'unsafe cleanup path' }
function Get-Action { param($Enabled, $NonInteractive, $Previous = '') (Get-InstallerGlobalPlan -Source $source -Target $target -Enabled $Enabled -NonInteractive $NonInteractive -PreviousSha $Previous).Action }
try {
    $source = Join-Path $scratch 'source.md'
    $target = Join-Path $scratch 'AGENTS.md'
    [IO.File]::WriteAllText($source, "new`n")
    if ((Get-Action $false $true) -ne 'skip') { throw 'disabled' }
    if ((Get-Action $true $true) -ne 'install') { throw 'missing' }
    [IO.File]::WriteAllText($target, "new`n")
    if ((Get-Action $true $true) -ne 'unchanged') { throw 'identical' }
    [IO.File]::WriteAllText($target, "old`n")
    if ((Get-Action $true $false) -ne 'confirm') { throw 'conflict needs confirmation' }
    $previous = Get-InstallerFileSha256 -Path $target
    if ((Get-Action $true $true $previous) -ne 'replace') { throw 'previously installed copy is replaceable' }
    $rejected = $false
    try { $null = Get-Action $true $true } catch { $rejected = $true }
    if (-not $rejected) { throw 'noninteractive conflict accepted' }
    if ([IO.File]::ReadAllText($target) -ne "old`n") { throw 'target changed' }
    Write-Output 'Windows Global Instructions policy cases passed.'
} finally {
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
