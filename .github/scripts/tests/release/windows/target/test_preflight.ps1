param([Parameter(Mandatory = $true)][string]$Binary)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/target/preflight.ps1')
$sourceWork = Join-Path $repoRoot 'skills/work'
$scratch = Join-Path $repoRoot ('.issue95-preflight.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolvedScratch = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolvedScratch.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolvedScratch) -match '^\.issue95-preflight\.[0-9a-f]{32}$')) { throw 'unsafe test cleanup path' }
function Assert-Kind([string] $Expected, [object] $Result) {
    if ($Result.Kind -ne $Expected) { throw "Expected $Expected, got $($Result.Kind): $($Result.Reason)" }
}
try {
    $target = Join-Path $scratch 'work'
    Assert-Kind 'new' (Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $false)
    New-Item -ItemType Directory -Path $target | Out-Null
    Assert-Kind 'new' (Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $false)
    foreach ($relative in @('SKILL.md', 'agents/openai.yaml', 'references/instruction-loading.md', 'references/instruction-loading/invocation.md', 'references/instructions/task/general/instructions.md', 'references/instructions/execute/general/instructions.md')) {
        $destination = Join-Path $target $relative
        $null = New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force
        Copy-Item -LiteralPath (Join-Path $sourceWork $relative) -Destination $destination
    }
    $null = New-Item -ItemType Directory -Path (Join-Path $target 'scripts')
    Set-Content -LiteralPath (Join-Path $target 'scripts/work.exe') -Value 'fixture'
    Assert-Kind 'unsafe' (Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $false)
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $target 'scripts/work.exe') -Force
    $firstUpdate = Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $false
    Assert-Kind 'first-update' $firstUpdate
    if (-not $firstUpdate.RequiresConfirmation) { throw 'first update must require confirmation' }
    if (Test-InstallerTargetAuthorization -Inspection $firstUpdate -NonInteractive $true -Confirmed $true) { throw 'non-interactive takeover' }
    if (Test-InstallerTargetAuthorization -Inspection $firstUpdate -NonInteractive $false -Confirmed $false) { throw 'unconfirmed takeover' }
    if (-not (Test-InstallerTargetAuthorization -Inspection $firstUpdate -NonInteractive $false -Confirmed $true)) { throw 'confirmed takeover' }
    Assert-Kind 'managed' (Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $true)
    Set-Content -LiteralPath (Join-Path $target 'SKILL.md') -Value 'unknown skill'
    Assert-Kind 'unsafe' (Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $false)
    Copy-Item -LiteralPath (Join-Path $sourceWork 'SKILL.md') -Destination (Join-Path $target 'SKILL.md') -Force
    Set-Content -LiteralPath (Join-Path $target 'unknown.txt') -Value 'user data'
    Assert-Kind 'unsafe' (Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $false)
    Remove-Item -LiteralPath (Join-Path $target 'unknown.txt')
    $link = Join-Path $target 'linked'
    $linkTarget = Join-Path $scratch 'link-target'
    $null = New-Item -ItemType Directory -Path $linkTarget
    $null = New-Item -ItemType Junction -Path $link -Target $linkTarget
    Assert-Kind 'unsafe' (Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $false)
    Remove-Item -LiteralPath $link -Force
    Remove-Item -LiteralPath (Join-Path $target 'references/instructions/execute/general/instructions.md')
    Assert-Kind 'unsafe' (Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $false)
    Write-Output 'Windows target preflight cases passed.'
} finally {
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolvedScratch) { Remove-Item -LiteralPath $resolvedScratch -Recurse -Force }
}
