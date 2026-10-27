Set-StrictMode -Version Latest

function Test-InstallerToolingTransactionSafe {
    param([string] $Transaction, [string] $Source)
    if (-not (Test-Path -LiteralPath $Transaction -PathType Container) -or
        ((Get-Item -LiteralPath $Transaction -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    foreach ($entry in @(Get-ChildItem -LiteralPath $Transaction -Force)) {
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
        if ($entry.Name -in @('target-path.txt', 'state-sha256.txt', 'verified.txt')) {
            if ($entry.PSIsContainer) { return $false }
        } elseif ($entry.Name -in @('prepared', 'previous', 'published-failed', 'published-interrupted')) {
            if (-not (Test-InstallerToolingSubset -Path $entry.FullName -Source $Source)) { return $false }
        } else { return $false }
    }
    return $true
}

function Remove-InstallerToolingTransactionSafe {
    param([string] $Transaction, [string] $Source)
    if (-not (Test-InstallerToolingTransactionSafe -Transaction $Transaction -Source $Source)) {
        throw "unsafe Tooling transaction: $Transaction"
    }
    foreach ($name in @('previous', 'prepared', 'published-failed', 'published-interrupted')) {
        $path = Join-Path $Transaction $name
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction Stop }
    }
    foreach ($name in @('target-path.txt', 'state-sha256.txt', 'verified.txt')) {
        $path = Join-Path $Transaction $name
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force -ErrorAction Stop }
    }
    Remove-Item -LiteralPath $Transaction -Force -ErrorAction Stop
}

function Get-InstallerToolingTransactions {
    param([string] $Target)
    $parent = Split-Path -Path $Target -Parent
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) { return @() }
    $records = @()
    foreach ($entry in @(Get-ChildItem -LiteralPath $parent -Directory -Force -ErrorAction SilentlyContinue)) {
        if ($entry.Name -cnotmatch '^\.tooling-install\.[0-9a-f]{32}$' -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        $record = Join-Path $entry.FullName 'target-path.txt'
        if ((Test-Path -LiteralPath $record -PathType Leaf) -and
            ((Get-Item -LiteralPath $record -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0 -and
            [IO.File]::ReadAllText($record).Trim() -ceq $Target) { $records += $entry.FullName }
    }
    return $records
}

function Restore-InstallerInterruptedTooling {
    param([string] $Target, [string] $Source, [string] $Version, [string] $Binary)
    $records = @(Get-InstallerToolingTransactions -Target $Target)
    if ($records.Count -gt 1) { throw 'multiple Tooling transactions claim target' }
    if ($records.Count -eq 0) { return }
    $transaction = $records[0]
    if (-not (Test-InstallerToolingTransactionSafe -Transaction $transaction -Source $Source)) {
        throw "unsafe Tooling recovery directory: $transaction"
    }
    $previous = Join-Path $transaction 'previous'
    if (Test-Path -LiteralPath (Join-Path $transaction 'verified.txt')) {
        if (-not (Test-InstallerToolingPayload -Path $Target -Source $Source -Target $Target -Version $Version -Binary $Binary) -or
            (Get-InstallerWorkFileSha256 (Join-Path $Target 'install-state.json')) -cne
                [IO.File]::ReadAllText((Join-Path $transaction 'state-sha256.txt')).Trim()) {
            throw "verified Tooling no longer matches recovery record: $transaction"
        }
        Remove-InstallerToolingTransactionSafe -Transaction $transaction -Source $Source
        return
    }
    if (Test-Path -LiteralPath $previous) {
        if (Test-Path -LiteralPath $Target) {
            if (-not (Test-InstallerToolingPayload -Path $Target -Source $Source -Target $Target -Version $Version -Binary $Binary)) {
                throw "unverified Tooling target cannot be identified: $Target"
            }
            Move-Item -LiteralPath $Target -Destination (Join-Path $transaction 'published-interrupted') -ErrorAction Stop
        }
        Move-Item -LiteralPath $previous -Destination $Target -ErrorAction Stop
    } elseif (-not (Test-Path -LiteralPath (Join-Path $transaction 'prepared')) -and (Test-Path -LiteralPath $Target)) {
        if (-not (Test-InstallerToolingPayload -Path $Target -Source $Source -Target $Target -Version $Version -Binary $Binary)) {
            throw "unverified Tooling target cannot be identified: $Target"
        }
        Move-Item -LiteralPath $Target -Destination (Join-Path $transaction 'published-interrupted') -ErrorAction Stop
    }
    Remove-InstallerToolingTransactionSafe -Transaction $transaction -Source $Source
}

function Invoke-InstallerToolingTransaction {
    param([string] $Target, [string] $Source, [string] $Version, [string] $Binary, [string] $StateJson,
        [bool] $NonInteractive, [bool] $Confirmed)
    Restore-InstallerInterruptedTooling -Target $Target -Source $Source -Version $Version -Binary $Binary
    $inspection = Inspect-InstallerToolingTarget -Target $Target -Source $Source
    if ($inspection.Kind -notin @('new', 'managed') -and
        -not ($inspection.Kind -eq 'first-update' -and -not $NonInteractive -and $Confirmed)) {
        throw "Tooling target is not authorized: $($inspection.Reason)"
    }
    $parent = Split-Path -Path $Target -Parent
    $transaction = Join-Path $parent ('.tooling-install.' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $transaction -ErrorAction Stop | Out-Null
    $prepared = Join-Path $transaction 'prepared'
    $previous = Join-Path $transaction 'previous'
    [IO.File]::WriteAllText((Join-Path $transaction 'target-path.txt'), $Target)
    New-Item -ItemType Directory -Path $prepared -ErrorAction Stop | Out-Null
    foreach ($entry in @(Get-ChildItem -LiteralPath $Source -Force)) {
        Copy-Item -LiteralPath $entry.FullName -Destination $prepared -Recurse -ErrorAction Stop
    }
    New-Item -ItemType Directory -Path (Join-Path $prepared 'scripts') -ErrorAction Stop | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $prepared 'scripts/tooling.exe') -ErrorAction Stop
    [IO.File]::WriteAllText((Join-Path $prepared 'install-state.json'), $StateJson, [Text.UTF8Encoding]::new($false))
    if (-not (Test-InstallerToolingPayload -Path $prepared -Source $Source -Target $Target -Version $Version -Binary $Binary)) {
        throw "Tooling preparation failed; recovery directory: $transaction"
    }
    [IO.File]::WriteAllText((Join-Path $transaction 'state-sha256.txt'),
        (Get-InstallerWorkFileSha256 (Join-Path $prepared 'install-state.json')))
    $inspection = Inspect-InstallerToolingTarget -Target $Target -Source $Source
    if ($inspection.Kind -notin @('new', 'managed') -and
        -not ($inspection.Kind -eq 'first-update' -and -not $NonInteractive -and $Confirmed)) {
        throw "Tooling target changed before publish: $($inspection.Reason)"
    }
    if (Test-Path -LiteralPath $Target) { Move-Item -LiteralPath $Target -Destination $previous -ErrorAction Stop }
    try {
        Move-Item -LiteralPath $prepared -Destination $Target -ErrorAction Stop
        if (-not (Test-InstallerToolingPayload -Path $Target -Source $Source -Target $Target -Version $Version -Binary $Binary)) {
            throw 'published Tooling failed validation'
        }
    } catch {
        $failure = $_
        if (Test-Path -LiteralPath $Target) {
            Move-Item -LiteralPath $Target -Destination (Join-Path $transaction 'published-failed') -ErrorAction Stop
        }
        if (Test-Path -LiteralPath $previous) {
            Move-Item -LiteralPath $previous -Destination $Target -ErrorAction Stop
        }
        throw $failure
    }
    [IO.File]::WriteAllText((Join-Path $transaction 'verified.txt'), 'verified')
    try { Remove-InstallerToolingTransactionSafe -Transaction $transaction -Source $Source }
    catch { Write-Warning "Verified Tooling retained; transaction awaits cleanup: $transaction. $_" }
    return $Target
}
