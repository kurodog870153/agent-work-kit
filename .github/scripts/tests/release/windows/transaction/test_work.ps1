$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/target/preflight.ps1')
function Test-InstallerLegacyWorkIdentity { param([string] $Target) return $true } # Transaction fixtures use text in place of the CLI binary.
. (Join-Path $repoRoot 'os-scripts/windows/installer/transaction/work.ps1')
. (Join-Path $repoRoot 'os-scripts/windows/installer/global-agents/policy.ps1')
. (Join-Path $repoRoot 'os-scripts/windows/installer/global-agents/transaction.ps1')
$sourceWork = Join-Path $repoRoot 'skills/work'
$scratch = Join-Path $repoRoot ('.issue95-transaction.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolvedScratch = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolvedScratch.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolvedScratch) -match '^\.issue95-transaction\.[0-9a-f]{32}$')) { throw 'unsafe test cleanup path' }
$target = Join-Path $scratch 'work'
$required = @('SKILL.md', 'agents/openai.yaml', 'references/instruction-loading.md',
    'references/instruction-loading/invocation.md', 'references/instructions/task/general/instructions.md',
    'references/instructions/execute/general/instructions.md')
$prepare = {
    param($prepared)
    foreach ($relative in $required) {
        $destination = Join-Path $prepared $relative
        $parent = Split-Path -Path $destination -Parent
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $sourceWork $relative) -Destination $destination
    }
    New-Item -ItemType Directory -Path (Join-Path $prepared 'scripts') | Out-Null
    [IO.File]::WriteAllText((Join-Path $prepared 'scripts/work.exe'), 'new')
    [IO.File]::WriteAllText((Join-Path $prepared 'install-state.json'), '{"version":"1.1.0"}')
}
$validate = {
    param($path)
    if ([IO.File]::ReadAllText((Join-Path $path 'scripts/work.exe')) -cne 'new') { throw 'binary smoke failed' }
}
try {
    $emptyTarget = Join-Path $scratch 'empty/work'
    New-Item -ItemType Directory -Path $emptyTarget -Force | Out-Null
    $null = Invoke-InstallerWorkTransaction -Target $emptyTarget -SourceWork $sourceWork -Prepare $prepare -Validate $validate -NonInteractive $true
    if (-not (Test-Path -LiteralPath (Join-Path $emptyTarget 'scripts/work.exe'))) { throw 'empty Work install failed' }
    $emptyRecovery = Join-Path $scratch 'empty-recovery/work'
    New-Item -ItemType Directory -Path $emptyRecovery -Force | Out-Null
    try {
        $null = Invoke-InstallerWorkTransaction -Target $emptyRecovery -SourceWork $sourceWork -Prepare $prepare -Validate $validate -NonInteractive $true -FailurePoint restore
        throw 'empty Work restore failure was accepted'
    } catch {
        if ($_.Exception.Message -eq 'empty Work restore failure was accepted') { throw }
    }
    Restore-InstallerInterruptedWork -Target $emptyRecovery -SourceWork $sourceWork
    if (-not (Test-Path -LiteralPath $emptyRecovery -PathType Container) -or
        @(Get-ChildItem -LiteralPath $emptyRecovery -Force).Count -ne 0) { throw 'empty Work was not restored' }
    $result = Invoke-InstallerWorkTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate
    if ($result -cne $target -or -not (Test-Path -LiteralPath $target)) { throw 'first install failed' }
    [IO.File]::WriteAllText((Join-Path $target 'scripts/work.exe'), 'old')
    foreach ($failure in @('replace', 'verify', 'restore')) {
        try {
            $null = Invoke-InstallerWorkTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -Confirmed $true -FailurePoint $failure
            throw "injected $failure failure was accepted"
        } catch {
            if ($_.Exception.Message -like 'injected * failure was accepted') { throw }
        }
        if ($failure -eq 'restore') {
            $null = Restore-InstallerInterruptedWork -Target $target -SourceWork $sourceWork
            if ([IO.File]::ReadAllText((Join-Path $target 'scripts/work.exe')) -cne 'old') { throw 'retry did not restore old Work' }
        } elseif ([IO.File]::ReadAllText((Join-Path $target 'scripts/work.exe')) -cne 'old') {
            throw "old Work was not restored after $failure"
        }
    }
    [IO.File]::WriteAllText((Join-Path $target 'unknown.txt'), 'keep')
    try {
        $null = Invoke-InstallerWorkTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -Confirmed $true
        throw 'unknown content was accepted'
    } catch {
        if ($_.Exception.Message -eq 'unknown content was accepted') { throw }
    }
    if ([IO.File]::ReadAllText((Join-Path $target 'unknown.txt')) -cne 'keep') { throw 'unknown content changed' }
    Remove-Item -LiteralPath (Join-Path $target 'unknown.txt')
    $result = Invoke-InstallerWorkTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -Confirmed $true
    if ([IO.File]::ReadAllText((Join-Path $target 'scripts/work.exe')) -cne 'new') { throw 'upgrade failed' }
    foreach ($interruptedPoint in @('staging', 'first-rename', 'second-rename', 'verified')) {
        [IO.File]::WriteAllText((Join-Path $target 'scripts/work.exe'), 'old')
        $transaction = Join-Path $scratch ('.work-install.' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $transaction | Out-Null
        [IO.File]::WriteAllText((Join-Path $transaction 'target-path.txt'), $target)
        $staged = Join-Path $transaction 'prepared'
        New-Item -ItemType Directory -Path $staged | Out-Null
        & $prepare $staged
        [IO.File]::WriteAllText((Join-Path $transaction 'state-sha256.txt'),
            (Get-InstallerWorkFileSha256 (Join-Path $staged 'install-state.json')))
        [IO.File]::WriteAllText((Join-Path $transaction 'binary-sha256.txt'),
            (Get-InstallerWorkFileSha256 (Join-Path $staged 'scripts/work.exe')))
        if ($interruptedPoint -ne 'staging') {
            Move-Item -LiteralPath $target -Destination (Join-Path $transaction 'previous')
        }
        if ($interruptedPoint -in @('second-rename', 'verified')) {
            Move-Item -LiteralPath $staged -Destination $target
        }
        if ($interruptedPoint -eq 'verified') {
            [IO.File]::WriteAllText((Join-Path $transaction 'verified.txt'), 'verified')
            Restore-InstallerInterruptedWork -Target $target -SourceWork $sourceWork
            if ([IO.File]::ReadAllText((Join-Path $target 'scripts/work.exe')) -cne 'new') {
                throw 'verified new Work was replaced'
            }
        } else {
            $null = Invoke-InstallerWorkTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -Confirmed $true
            if ([IO.File]::ReadAllText((Join-Path $target 'scripts/work.exe')) -cne 'new') {
                throw "new Work missing after $interruptedPoint"
            }
        }
        if (Test-Path -LiteralPath $transaction) { throw "transaction retained after $interruptedPoint" }
    }
    [IO.File]::WriteAllText((Join-Path $target 'scripts/work.exe'), 'old')
    $result = Invoke-InstallerWorkTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -Confirmed $true -FailurePoint cleanup
    if ($result -cne $target -or [IO.File]::ReadAllText((Join-Path $target 'scripts/work.exe')) -cne 'new') {
        throw 'cleanup failure marked installation failed or lost verified Work'
    }
    Restore-InstallerInterruptedWork -Target $target -SourceWork $sourceWork
    if ([IO.File]::ReadAllText((Join-Path $target 'scripts/work.exe')) -cne 'new') {
        throw 'pending cleanup replaced verified Work'
    }
    $globalSource = Join-Path $scratch 'global-source.md'
    $globalTarget = Join-Path $scratch 'AGENTS.md'
    [IO.File]::WriteAllText($globalSource, "new global`n")
    [IO.File]::WriteAllText($globalTarget, "old global`r`n")
    $originalGlobal = [IO.File]::ReadAllBytes($globalTarget)
    $plan = Get-InstallerGlobalAgentsPlan -Source $globalSource -Target $globalTarget -Enabled $true -NonInteractive $false -Confirmed $true
    try {
        $null = Invoke-InstallerCombinedTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -Confirmed $true -GlobalPlan $plan -GlobalSource $globalSource -GlobalTarget $globalTarget -WorkFailurePoint replace
        throw 'combined Work failure was accepted'
    } catch {
        if ($_.Exception.Message -eq 'combined Work failure was accepted') { throw }
    }
    if (-not [Linq.Enumerable]::SequenceEqual([byte[]][IO.File]::ReadAllBytes($globalTarget), [byte[]]$originalGlobal)) {
        throw 'Global AGENTS bytes changed after Work failure'
    }
    $null = Invoke-InstallerCombinedTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -Confirmed $true -GlobalPlan $plan -GlobalSource $globalSource -GlobalTarget $globalTarget
    if ([IO.File]::ReadAllText($globalTarget) -cne "new global`n") { throw 'combined Global AGENTS publish failed' }
    Write-Output 'Windows Work transaction cases passed.'
} finally {
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolvedScratch) { Remove-Item -LiteralPath $resolvedScratch -Recurse -Force }
}
