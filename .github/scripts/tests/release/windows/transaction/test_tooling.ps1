$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/install/tooling-install.ps1')
. (Join-Path $repoRoot 'os-scripts/windows/installer/transaction/tooling.ps1')

$scratch = Join-Path $repoRoot ('.tooling-transaction-test.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolvedScratch = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolvedScratch.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolvedScratch) -match '^\.tooling-transaction-test\.[0-9a-f]{32}$')) {
    throw 'unsafe test cleanup path'
}
try {
    $target = Join-Path $scratch 'tooling'
    $source = Join-Path $repoRoot 'skills/tooling'
    $transaction = Join-Path $scratch ('.tooling-install.' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $transaction | Out-Null
    [IO.File]::WriteAllText((Join-Path $transaction 'target-path.txt'), $target)
    if (-not (Test-InstallerToolingTransactionSafe -Transaction $transaction -Source $source)) {
        throw 'valid Tooling transaction was rejected'
    }
    if (@(Get-InstallerToolingTransactions -Target $target).Count -ne 1) {
        throw 'pending Tooling transaction was not found'
    }
    [IO.File]::WriteAllText((Join-Path $transaction 'unknown.txt'), 'keep')
    if (Test-InstallerToolingTransactionSafe -Transaction $transaction -Source $source) {
        throw 'unsafe Tooling transaction was accepted'
    }
    try {
        Remove-InstallerToolingTransactionSafe -Transaction $transaction -Source $source
        throw 'unsafe Tooling transaction was removed'
    } catch {
        if ($_.Exception.Message -eq 'unsafe Tooling transaction was removed') { throw }
    }
    if ([IO.File]::ReadAllText((Join-Path $transaction 'unknown.txt')) -cne 'keep') {
        throw 'unsafe Tooling transaction content changed'
    }
    Remove-Item -LiteralPath (Join-Path $transaction 'unknown.txt') -Force
    Remove-InstallerToolingTransactionSafe -Transaction $transaction -Source $source
    if (Test-Path -LiteralPath $transaction) { throw 'Tooling transaction cleanup failed' }
    Write-Output 'Windows Tooling transaction cases passed.'
} finally {
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolvedScratch) {
        Remove-Item -LiteralPath $resolvedScratch -Recurse -Force
    }
}
