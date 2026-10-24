Set-StrictMode -Version Latest

function Invoke-InstallerGlobalAgentsStage {
    param([object] $Plan, [string] $Source, [string] $Target, [string] $Transaction,
        [string] $FailurePoint = '')
    if ($Plan.Action -in @('skip', 'unchanged')) { return $false }
    if ($Plan.Action -notin @('install', 'replace')) { throw 'Global AGENTS replacement is not confirmed' }
    if ((Get-InstallerFileSha256 -Path $Source) -cne $Plan.SourceSha256) {
        throw 'Global AGENTS source changed after confirmation'
    }
    if (-not (Test-Path -LiteralPath $Transaction -PathType Container)) { throw 'transaction directory is missing' }
    if (-not (Test-Path -LiteralPath (Split-Path -Path $Target -Parent) -PathType Container)) {
        throw 'Global AGENTS target parent is missing'
    }
    $prepared = Join-Path $Transaction 'global-prepared'
    $previous = Join-Path $Transaction 'global-previous'
    if ((Test-Path -LiteralPath $prepared) -or (Test-Path -LiteralPath $previous)) { throw 'Global AGENTS stage already exists' }
    Copy-Item -LiteralPath $Source -Destination $prepared -ErrorAction Stop
    if ((Get-InstallerFileSha256 -Path $prepared) -cne $Plan.SourceSha256) {
        throw 'staged Global AGENTS bytes differ from source'
    }
    $exists = Test-Path -LiteralPath $Target
    if ($Plan.Action -eq 'replace') {
        if (-not $exists) { throw 'Global AGENTS target vanished after confirmation' }
        $item = Get-Item -LiteralPath $Target -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            (Get-InstallerFileSha256 -Path $Target) -cne $Plan.TargetSha256) {
            throw 'Global AGENTS target changed after confirmation'
        }
        Move-Item -LiteralPath $Target -Destination $previous -ErrorAction Stop
    } elseif ($exists) {
        throw 'Global AGENTS target appeared after preflight'
    }
    try {
        if ($FailurePoint -in @('publish', 'restore')) { throw 'injected Global AGENTS publish failure' }
        Move-Item -LiteralPath $prepared -Destination $Target -ErrorAction Stop
        if ((Get-InstallerFileSha256 -Path $Target) -cne $Plan.SourceSha256) {
            throw 'published Global AGENTS bytes differ from source'
        }
        return $true
    } catch {
        $failure = $_
        if (Test-Path -LiteralPath $Target) {
            Move-Item -LiteralPath $Target -Destination (Join-Path $Transaction 'global-failed') -ErrorAction Stop
        }
        if (Test-Path -LiteralPath $previous) {
            if ($FailurePoint -eq 'restore') { throw "Global AGENTS restore failed; previous bytes remain at $previous" }
            Move-Item -LiteralPath $previous -Destination $Target -ErrorAction Stop
        }
        throw $failure
    }
}

function Undo-InstallerGlobalAgentsStage {
    param([object] $Plan, [string] $Target, [string] $Transaction)
    if ($Plan.Action -in @('skip', 'unchanged')) { return }
    $previous = Join-Path $Transaction 'global-previous'
    if (Test-Path -LiteralPath $Target) {
        $item = Get-Item -LiteralPath $Target -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            (Get-InstallerFileSha256 -Path $Target) -cne $Plan.SourceSha256) {
            throw "Global AGENTS changed during rollback; preserved at $Target"
        }
        Move-Item -LiteralPath $Target -Destination (Join-Path $Transaction 'global-undone') -ErrorAction Stop
    }
    if ($Plan.Action -eq 'replace') {
        if (-not (Test-Path -LiteralPath $previous)) { throw 'Global AGENTS previous bytes are missing' }
        Move-Item -LiteralPath $previous -Destination $Target -ErrorAction Stop
        if ((Get-InstallerFileSha256 -Path $Target) -cne $Plan.TargetSha256) {
            throw 'Global AGENTS rollback bytes differ from original'
        }
    }
}

function Write-InstallerGlobalTransactionRecord {
    param([string] $Transaction, [string] $WorkTarget, [string] $GlobalTarget, [object] $Plan)
    if ($Plan.Action -notin @('install', 'replace')) { throw 'invalid Global AGENTS transaction action' }
    $record = [pscustomobject]@{
        work_target = $WorkTarget; global_target = $GlobalTarget; action = $Plan.Action
        source_sha256 = $Plan.SourceSha256
        previous_sha256 = $(if ($Plan.Action -eq 'replace') { $Plan.TargetSha256 } else { '-' })
    }
    [IO.File]::WriteAllText((Join-Path $Transaction 'record.json'), ($record | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
}

function Write-InstallerGlobalWorkVerified {
    param([string] $Transaction, [string] $WorkTransaction)
    if (-not (Test-Path -LiteralPath (Join-Path $WorkTransaction 'verified.txt') -PathType Leaf)) {
        throw 'Work has no final verification record'
    }
    $hashes = @('state-sha256.txt', 'binary-sha256.txt') | ForEach-Object {
        [IO.File]::ReadAllText((Join-Path $WorkTransaction $_)).Trim()
    }
    if (@($hashes | Where-Object { $_ -cnotmatch '^[0-9a-f]{64}$' }).Count -ne 0) {
        throw 'invalid verified Work hashes'
    }
    $temporary = Join-Path $Transaction 'work-verified.tmp'
    if (Test-Path -LiteralPath $temporary) { throw 'temporary verification record already exists' }
    [IO.File]::WriteAllLines($temporary, [string[]]$hashes, [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination (Join-Path $Transaction 'work-verified.txt') -ErrorAction Stop
}

function Get-InstallerGlobalTransactionRecord {
    param([string] $Transaction)
    if (-not (Test-Path -LiteralPath $Transaction -PathType Container)) { throw 'Global transaction is missing' }
    $root = Get-Item -LiteralPath $Transaction -Force
    if (($root.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Global transaction is a link' }
    $recordPath = Join-Path $Transaction 'record.json'
    if (-not (Test-Path -LiteralPath $recordPath -PathType Leaf)) { throw 'Global transaction record is missing' }
    if (((Get-Item -LiteralPath $recordPath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'Global transaction record is a link'
    }
    $record = [IO.File]::ReadAllText($recordPath) | ConvertFrom-Json
    foreach ($field in @('work_target', 'global_target', 'action', 'source_sha256', 'previous_sha256')) {
        if ($null -eq $record -or $field -notin $record.PSObject.Properties.Name) {
            throw 'invalid Global transaction record'
        }
    }
    if (-not [IO.Path]::IsPathRooted($record.work_target) -or
        -not [IO.Path]::IsPathRooted($record.global_target) -or
        $record.source_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        ($record.action -eq 'install' -and $record.previous_sha256 -cne '-') -or
        ($record.action -eq 'replace' -and $record.previous_sha256 -cnotmatch '^[0-9a-f]{64}$') -or
        $record.action -notin @('install', 'replace')) { throw 'invalid Global transaction record' }
    $probe = Split-Path -Path $record.global_target -Parent
    while (-not [string]::IsNullOrEmpty($probe)) {
        if (Test-Path -LiteralPath $probe) {
            $item = Get-Item -LiteralPath $probe -Force
            if (-not $item.PSIsContainer -or
                ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'unsafe Global target path' }
        }
        $parent = Split-Path -Path $probe -Parent
        if ($parent -eq $probe) { break }
        $probe = $parent
    }
    foreach ($entry in @(Get-ChildItem -LiteralPath $Transaction -Force)) {
        if ($entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "unsafe Global transaction entry: $($entry.FullName)"
        }
        switch ($entry.Name) {
            'record.json' { }
            { $_ -in @('work-verified.txt', 'work-verified.tmp') } {
                $lines = [IO.File]::ReadAllLines($entry.FullName)
                if ($lines.Count -ne 2 -or $lines[0] -cnotmatch '^[0-9a-f]{64}$' -or
                    $lines[1] -cnotmatch '^[0-9a-f]{64}$') { throw 'invalid Work verification record' }
            }
            'global-previous' {
                if ((Get-InstallerFileSha256 -Path $entry.FullName) -cne $record.previous_sha256) {
                    throw 'previous Global AGENTS changed'
                }
            }
            { $_ -in @('global-prepared', 'global-failed', 'global-undone') } {
                if ((Get-InstallerFileSha256 -Path $entry.FullName) -cne $record.source_sha256) {
                    throw 'staged Global AGENTS changed'
                }
            }
            default { throw "unknown Global transaction entry: $($entry.Name)" }
        }
    }
    return $record
}

function Remove-InstallerGlobalTransactionSafe {
    param([string] $Transaction)
    $null = Get-InstallerGlobalTransactionRecord -Transaction $Transaction
    foreach ($entry in @(Get-ChildItem -LiteralPath $Transaction -Force)) {
        if ($entry.Name -in @('record.json', 'work-verified.txt')) { continue }
        Remove-Item -LiteralPath $entry.FullName -Force -ErrorAction Stop
    }
    Remove-Item -LiteralPath (Join-Path $Transaction 'record.json') -Force -ErrorAction Stop
    if (Test-Path -LiteralPath (Join-Path $Transaction 'work-verified.txt')) {
        Remove-Item -LiteralPath (Join-Path $Transaction 'work-verified.txt') -Force -ErrorAction Stop
    }
    Remove-Item -LiteralPath $Transaction -Force -ErrorAction Stop
}

function Test-InstallerGlobalTransactionPending {
    param([string] $WorkTarget)
    $parent = Split-Path -Path $WorkTarget -Parent
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) { return $false }
    foreach ($entry in @(Get-ChildItem -LiteralPath $parent -Directory -Force)) {
        if ($entry.Name -cnotmatch '^\.agents-install\.[0-9a-f]{32}$' -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        $path = Join-Path $entry.FullName 'record.json'
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
        if (((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        try { if (([IO.File]::ReadAllText($path) | ConvertFrom-Json).work_target -ceq $WorkTarget) { return $true } }
        catch { }
    }
    return $false
}

function Restore-InstallerInterruptedGlobal {
    param([string] $WorkTarget, [string] $SourceWork)
    $parent = Split-Path -Path $WorkTarget -Parent
    $transactions = @()
    foreach ($entry in @(Get-ChildItem -LiteralPath $parent -Directory -Force)) {
        if ($entry.Name -cnotmatch '^\.agents-install\.[0-9a-f]{32}$' -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        $path = Join-Path $entry.FullName 'record.json'
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
        if (((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "unsafe Global transaction record: $path"
        }
        $record = [IO.File]::ReadAllText($path) | ConvertFrom-Json
        if ($null -eq $record -or 'work_target' -notin $record.PSObject.Properties.Name) {
            throw 'invalid Global transaction record'
        }
        if ($record.work_target -ceq $WorkTarget) { $transactions += $entry.FullName }
    }
    if ($transactions.Count -gt 1) { throw 'multiple Global AGENTS transactions claim Work' }
    if ($transactions.Count -eq 0) { return }
    $transaction = $transactions[0]
    $record = Get-InstallerGlobalTransactionRecord -Transaction $transaction
    $workTransactions = @()
    foreach ($entry in @(Get-ChildItem -LiteralPath $parent -Directory -Force)) {
        if ($entry.Name -cnotmatch '^\.work-install\.[0-9a-f]{32}$' -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        $path = Join-Path $entry.FullName 'target-path.txt'
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            if ([IO.File]::ReadAllText($path) -ceq $WorkTarget) { $workTransactions += $entry.FullName }
        }
    }
    if ($workTransactions.Count -gt 1) { throw 'multiple Work transactions claim target' }
    $workTransaction = if ($workTransactions.Count -eq 1) { $workTransactions[0] } else { $null }
    $verified = Join-Path $transaction 'work-verified.txt'
    if (-not (Test-Path -LiteralPath $verified) -and $null -ne $workTransaction -and
        (Test-Path -LiteralPath (Join-Path $workTransaction 'verified.txt'))) {
        if (-not (Test-InstallerWorkMatchesRecord -Target $WorkTarget -SourceWork $SourceWork -Transaction $workTransaction)) {
            throw 'verified Work does not match combined transaction'
        }
        Write-InstallerGlobalWorkVerified -Transaction $transaction -WorkTransaction $workTransaction
    }
    if (Test-Path -LiteralPath $verified) {
        if (-not (Test-InstallerWorkDirectory -Path $WorkTarget -SourceWork $SourceWork)) {
            throw 'verified Work directory is unsafe'
        }
        $hashes = [IO.File]::ReadAllLines($verified)
        if ((Get-InstallerWorkFileSha256 (Join-Path $WorkTarget 'install-state.json')) -cne $hashes[0] -or
            (Get-InstallerWorkFileSha256 (Join-Path $WorkTarget 'scripts/work.exe')) -cne $hashes[1] -or
            -not (Test-Path -LiteralPath $record.global_target -PathType Leaf) -or
            (Get-InstallerFileSha256 -Path $record.global_target) -cne $record.source_sha256) {
            throw 'verified combined transaction changed'
        }
        try { Remove-InstallerGlobalTransactionSafe -Transaction $transaction }
        catch { Write-Warning "Verified Global AGENTS retained; transaction awaits cleanup: $transaction. $_" }
        return
    }
    $globalTarget = $record.global_target
    $previous = Join-Path $transaction 'global-previous'
    if (Test-Path -LiteralPath $globalTarget) {
        $item = Get-Item -LiteralPath $globalTarget -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'unsafe Global AGENTS target'
        }
        $hash = Get-InstallerFileSha256 -Path $globalTarget
        if ($record.action -eq 'replace' -and -not (Test-Path -LiteralPath $previous) -and
            $hash -ceq $record.previous_sha256) {
            # Stage did not change the existing target.
        } elseif ($hash -ceq $record.source_sha256) {
            $plan = [pscustomobject]@{ Action = $record.action; SourceSha256 = $record.source_sha256;
                TargetSha256 = $record.previous_sha256 }
            Undo-InstallerGlobalAgentsStage -Plan $plan -Target $globalTarget -Transaction $transaction
        } else { throw 'Global AGENTS changed after interruption' }
    } elseif ($record.action -eq 'replace' -and (Test-Path -LiteralPath $previous)) {
        $plan = [pscustomobject]@{ Action = $record.action; SourceSha256 = $record.source_sha256;
            TargetSha256 = $record.previous_sha256 }
        Undo-InstallerGlobalAgentsStage -Plan $plan -Target $globalTarget -Transaction $transaction
    } elseif ($record.action -eq 'replace') { throw 'previous Global AGENTS is missing' }
    Remove-InstallerGlobalTransactionSafe -Transaction $transaction
}
