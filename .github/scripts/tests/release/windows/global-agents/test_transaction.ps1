$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/global-agents/policy.ps1')
. (Join-Path $repoRoot 'os-scripts/windows/installer/global-agents/transaction.ps1')
$scratch = Join-Path $repoRoot ('.issue95-global-transaction.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolved = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolved.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolved) -match '^\.issue95-global-transaction\.[0-9a-f]{32}$')) { throw 'unsafe cleanup path' }
try {
    $source = Join-Path $scratch 'source.md'
    $target = Join-Path $scratch 'AGENTS.md'
    [IO.File]::WriteAllText($source, "new`n")
    [IO.File]::WriteAllText($target, "old`r`n")
    $original = [IO.File]::ReadAllBytes($target)
    $plan = Get-InstallerGlobalAgentsPlan -Source $source -Target $target -Enabled $true -NonInteractive $false -Confirmed $true
    foreach ($failure in @('publish', 'restore')) {
        $transaction = Join-Path $scratch ([guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $transaction | Out-Null
        try {
            $null = Invoke-InstallerGlobalAgentsStage -Plan $plan -Source $source -Target $target -Transaction $transaction -FailurePoint $failure
            throw "injected $failure failure accepted"
        } catch {
            if ($_.Exception.Message -like 'injected * failure accepted') { throw }
        }
        if ($failure -eq 'restore') {
            $previous = Join-Path $transaction 'global-previous'
            if (-not (Test-Path -LiteralPath $previous)) { throw 'missing recovery bytes' }
            Move-Item -LiteralPath $previous -Destination $target
        }
        if (-not [Linq.Enumerable]::SequenceEqual([byte[]][IO.File]::ReadAllBytes($target), [byte[]]$original)) {
            throw "original bytes lost after $failure"
        }
    }
    $transaction = Join-Path $scratch 'successful'
    New-Item -ItemType Directory -Path $transaction | Out-Null
    $result = Invoke-InstallerGlobalAgentsStage -Plan $plan -Source $source -Target $target -Transaction $transaction
    if (-not $result -or [IO.File]::ReadAllText($target) -cne "new`n") { throw 'replacement failed' }
    Undo-InstallerGlobalAgentsStage -Plan $plan -Target $target -Transaction $transaction
    if (-not [Linq.Enumerable]::SequenceEqual([byte[]][IO.File]::ReadAllBytes($target), [byte[]]$original)) { throw 'rollback changed original bytes' }
    Remove-Item -LiteralPath $target
    $plan = Get-InstallerGlobalAgentsPlan -Source $source -Target $target -Enabled $true -NonInteractive $true -Confirmed $false
    $transaction = Join-Path $scratch 'new-install'
    New-Item -ItemType Directory -Path $transaction | Out-Null
    $null = Invoke-InstallerGlobalAgentsStage -Plan $plan -Source $source -Target $target -Transaction $transaction
    Undo-InstallerGlobalAgentsStage -Plan $plan -Target $target -Transaction $transaction
    if (Test-Path -LiteralPath $target) { throw 'new file was not rolled back' }
    Write-Output 'Windows Global AGENTS transaction cases passed.'
} finally {
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
