Set-StrictMode -Version Latest

function Test-InstallerWorkDirectory {
    param([string] $Path, [string] $SourceWork)
    $inspection = Inspect-InstallerTarget -Target $Path -SourceWork $SourceWork -StateValid $false
    return ($inspection.Kind -in @('managed', 'first-update') -or
        ($inspection.Kind -eq 'new' -and (Test-Path -LiteralPath $Path -PathType Container)))
}

function Get-InstallerWorkFileSha256 {
    param([string] $Path)
    $stream = [IO.File]::OpenRead($Path)
    try {
        $algorithm = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
        finally { $algorithm.Dispose() }
    } finally { $stream.Dispose() }
}

function Test-InstallerWorkSubset {
    param([string] $Path, [string] $SourceWork)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return $false }
    $root = Get-Item -LiteralPath $Path -Force
    if (($root.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    $stack = [System.Collections.Generic.Stack[object]]::new()
    foreach ($child in @(Get-ChildItem -LiteralPath $Path -Force)) { $stack.Push($child) }
    while ($stack.Count -gt 0) {
        $entry = $stack.Pop()
        $relative = $entry.FullName.Substring($Path.TrimEnd('\', '/').Length + 1).Replace('\', '/')
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            -not (Test-InstallerManagedRelative $relative)) { return $false }
        if ($relative -notin @('scripts', 'scripts/work.exe', 'install-state.json') -and
            -not (Test-Path -LiteralPath (Join-Path $SourceWork $relative))) { return $false }
        if ($entry.PSIsContainer) {
            foreach ($child in @(Get-ChildItem -LiteralPath $entry.FullName -Force)) { $stack.Push($child) }
        }
    }
    return $true
}

function Test-InstallerWorkTransactionSafe {
    param([string] $Transaction, [string] $SourceWork)
    if (-not (Test-Path -LiteralPath $Transaction -PathType Container)) { return $false }
    $root = Get-Item -LiteralPath $Transaction -Force
    if (($root.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    foreach ($entry in @(Get-ChildItem -LiteralPath $Transaction -Force)) {
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
        if ($entry.Name -in @('target-path.txt', 'state-sha256.txt', 'binary-sha256.txt', 'verified.txt')) {
            if ($entry.PSIsContainer) { return $false }
        } elseif ($entry.Name -in @('prepared', 'previous', 'published-failed', 'published-interrupted')) {
            if (-not (Test-InstallerWorkSubset -Path $entry.FullName -SourceWork $SourceWork)) { return $false }
        } else { return $false }
    }
    return $true
}

function Test-InstallerWorkMatchesRecord {
    param([string] $Target, [string] $SourceWork, [string] $Transaction)
    if (-not (Test-InstallerWorkDirectory -Path $Target -SourceWork $SourceWork)) { return $false }
    $state = Join-Path $Target 'install-state.json'
    $binary = Join-Path $Target 'scripts/work.exe'
    $stateRecord = Join-Path $Transaction 'state-sha256.txt'
    $binaryRecord = Join-Path $Transaction 'binary-sha256.txt'
    foreach ($path in @($state, $binary, $stateRecord, $binaryRecord)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or
            ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            return $false
        }
    }
    return ((Get-InstallerWorkFileSha256 $state) -ceq [IO.File]::ReadAllText($stateRecord).Trim() -and
        (Get-InstallerWorkFileSha256 $binary) -ceq [IO.File]::ReadAllText($binaryRecord).Trim())
}

function Remove-InstallerWorkTransactionSafe {
    param([string] $Transaction, [string] $SourceWork)
    if (-not (Test-InstallerWorkTransactionSafe -Transaction $Transaction -SourceWork $SourceWork)) {
        throw "unsafe Work transaction: $Transaction"
    }
    foreach ($name in @('previous', 'prepared', 'published-failed', 'published-interrupted')) {
        $path = Join-Path $Transaction $name
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction Stop
        }
    }
    foreach ($name in @('state-sha256.txt', 'binary-sha256.txt', 'verified.txt', 'target-path.txt')) {
        $path = Join-Path $Transaction $name
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Force -ErrorAction Stop
        }
    }
    Remove-Item -LiteralPath $Transaction -Force -ErrorAction Stop
}

function Test-InstallerWorkTransactionPending {
    param([string] $Target)
    $parent = Split-Path -Path $Target -Parent
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) { return $false }
    foreach ($entry in @(Get-ChildItem -LiteralPath $parent -Directory -Force -ErrorAction SilentlyContinue)) {
        if ($entry.Name -cnotmatch '^\.work-install\.[0-9a-f]{32}$' -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        $record = Join-Path $entry.FullName 'target-path.txt'
        if (Test-Path -LiteralPath $record -PathType Leaf) {
            $recordItem = Get-Item -LiteralPath $record -Force
            if (($recordItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0 -and
                [IO.File]::ReadAllText($record) -ceq $Target) { return $true }
        }
    }
    return $false
}

function Restore-InstallerInterruptedWork {
    param([string] $Target, [string] $SourceWork)
    $parent = Split-Path -Path $Target -Parent
    $records = @()
    foreach ($entry in @(Get-ChildItem -LiteralPath $parent -Directory -Force -ErrorAction SilentlyContinue)) {
        if ($entry.Name -cnotmatch '^\.work-install\.[0-9a-f]{32}$' -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
        $record = Join-Path $entry.FullName 'target-path.txt'
        if (-not (Test-Path -LiteralPath $record -PathType Leaf)) { continue }
        if (((Get-Item -LiteralPath $record -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "unsafe transaction record: $record"
        }
        if (([IO.File]::ReadAllText($record) -ceq $Target)) { $records += $entry.FullName }
    }
    if ($records.Count -gt 1) { throw 'multiple recovery directories claim the same Work target' }
    if ($records.Count -eq 0) { return }
    $transaction = $records[0]
    if (-not (Test-InstallerWorkTransactionSafe -Transaction $transaction -SourceWork $SourceWork)) {
        throw "unsafe Work recovery directory: $transaction"
    }
    $previous = Join-Path $transaction 'previous'
    if (Test-Path -LiteralPath (Join-Path $transaction 'verified.txt')) {
        if (-not (Test-InstallerWorkMatchesRecord -Target $Target -SourceWork $SourceWork -Transaction $transaction)) {
            throw "verified Work no longer matches recovery record: $transaction"
        }
        try { Remove-InstallerWorkTransactionSafe -Transaction $transaction -SourceWork $SourceWork }
        catch { Write-Warning "Verified Work retained; old transaction awaits cleanup: $transaction. $_" }
        return
    }
    if (Test-Path -LiteralPath $previous) {
        if (-not (Test-InstallerWorkDirectory -Path $previous -SourceWork $SourceWork)) {
            throw "recovery Work directory is unsafe: $previous"
        }
        if (Test-Path -LiteralPath $Target) {
            if (-not (Test-InstallerWorkMatchesRecord -Target $Target -SourceWork $SourceWork -Transaction $transaction)) {
                throw "unverified final Work cannot be identified: $Target"
            }
            $interrupted = Join-Path $transaction 'published-interrupted'
            if (Test-Path -LiteralPath $interrupted) { throw "ambiguous recovery directory: $interrupted" }
            Move-Item -LiteralPath $Target -Destination $interrupted -ErrorAction Stop
        }
        try { Move-Item -LiteralPath $previous -Destination $Target -ErrorAction Stop }
        catch { throw "Work restore failed; previous installation remains at $previous. $_" }
    } elseif (-not (Test-Path -LiteralPath (Join-Path $transaction 'prepared')) -and
        (Test-Path -LiteralPath $Target) -and
        (Test-InstallerWorkMatchesRecord -Target $Target -SourceWork $SourceWork -Transaction $transaction)) {
        $interrupted = Join-Path $transaction 'published-interrupted'
        if (Test-Path -LiteralPath $interrupted) { throw "ambiguous recovery directory: $interrupted" }
        Move-Item -LiteralPath $Target -Destination $interrupted -ErrorAction Stop
    } elseif (-not (Test-Path -LiteralPath (Join-Path $transaction 'prepared')) -and
        -not (Test-Path -LiteralPath (Join-Path $transaction 'published-failed')) -and
        -not (Test-Path -LiteralPath (Join-Path $transaction 'published-interrupted')) -and
        (Test-Path -LiteralPath $Target)) {
        throw "unverified first installation cannot be identified: $Target"
    }
    Remove-InstallerWorkTransactionSafe -Transaction $transaction -SourceWork $SourceWork
}

function Invoke-InstallerWorkTransaction {
    param(
        [Parameter(Mandatory = $true)][string] $Target,
        [Parameter(Mandatory = $true)][string] $SourceWork,
        [Parameter(Mandatory = $true)][scriptblock] $Prepare,
        [Parameter(Mandatory = $true)][scriptblock] $Validate,
        [bool] $StateValid = $false,
        [bool] $Confirmed = $false,
        [bool] $NonInteractive = $false,
        [string] $FailurePoint = '',
        [bool] $DeferCleanup = $false
    )
    if (-not [IO.Path]::IsPathRooted($Target) -or [IO.Path]::GetFileName($Target) -cne 'work') {
        throw 'Work target must be an absolute work directory'
    }
    $targetPath = [IO.Path]::GetFullPath($Target)
    $parent = Split-Path -Path $targetPath -Parent
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) { throw "Work parent does not exist: $parent" }
    Restore-InstallerInterruptedWork -Target $targetPath -SourceWork $SourceWork
    $inspection = Inspect-InstallerTarget -Target $targetPath -SourceWork $SourceWork -StateValid $StateValid
    if (-not (Test-InstallerTargetAuthorization -Inspection $inspection -NonInteractive $NonInteractive -Confirmed $Confirmed)) {
        throw "Work target is not authorized: $($inspection.Reason)"
    }
    $transaction = Join-Path $parent ('.work-install.' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $transaction -ErrorAction Stop | Out-Null
    $prepared = Join-Path $transaction 'prepared'
    $previous = Join-Path $transaction 'previous'
    [IO.File]::WriteAllText((Join-Path $transaction 'target-path.txt'), $targetPath)
    New-Item -ItemType Directory -Path $prepared -ErrorAction Stop | Out-Null
    try {
        & $Prepare $prepared
        & $Validate $prepared
        if (-not (Test-InstallerWorkDirectory -Path $prepared -SourceWork $SourceWork)) {
            throw 'prepared Work directory failed structural validation'
        }
        if ($FailurePoint -eq 'prepare') { throw 'injected preparation failure' }
        [IO.File]::WriteAllText((Join-Path $transaction 'state-sha256.txt'),
            (Get-InstallerWorkFileSha256 (Join-Path $prepared 'install-state.json')))
        [IO.File]::WriteAllText((Join-Path $transaction 'binary-sha256.txt'),
            (Get-InstallerWorkFileSha256 (Join-Path $prepared 'scripts/work.exe')))
        $inspection = Inspect-InstallerTarget -Target $targetPath -SourceWork $SourceWork -StateValid $StateValid
        if (-not (Test-InstallerTargetAuthorization -Inspection $inspection -NonInteractive $NonInteractive -Confirmed $Confirmed)) {
            throw "Work target changed before publish: $($inspection.Reason)"
        }
        if (Test-Path -LiteralPath $targetPath) {
            Move-Item -LiteralPath $targetPath -Destination $previous -ErrorAction Stop
        }
        try {
            if ($FailurePoint -in @('replace', 'restore')) { throw 'injected replacement failure' }
            Move-Item -LiteralPath $prepared -Destination $targetPath -ErrorAction Stop
            & $Validate $targetPath
            if (-not (Test-InstallerWorkDirectory -Path $targetPath -SourceWork $SourceWork)) {
                throw 'published Work directory failed structural validation'
            }
            if ($FailurePoint -eq 'verify') { throw 'injected final verification failure' }
        } catch {
            $publishError = $_
            if (Test-Path -LiteralPath $targetPath) {
                $failed = Join-Path $transaction 'published-failed'
                Move-Item -LiteralPath $targetPath -Destination $failed -ErrorAction Stop
            }
            if (Test-Path -LiteralPath $previous) {
                if ($FailurePoint -eq 'restore') { throw "Work restore failed; previous installation remains at $previous" }
                Move-Item -LiteralPath $previous -Destination $targetPath -ErrorAction Stop
            }
            throw $publishError
        }
        [IO.File]::WriteAllText((Join-Path $transaction 'verified.txt'), 'verified')
        if ($DeferCleanup) {
            $script:InstallerLastWorkTransaction = $transaction
            return $targetPath
        }
        try {
            if ($FailurePoint -eq 'cleanup') { throw 'injected cleanup failure' }
            Remove-InstallerWorkTransactionSafe -Transaction $transaction -SourceWork $SourceWork
        }
        catch { Write-Warning "Work installed; old transaction awaits cleanup: $transaction. $_" }
        return $targetPath
    } catch {
        throw "Work transaction failed; recovery directory: $transaction. $_"
    }
}

function Invoke-InstallerCombinedTransaction {
    param(
        [string] $Target, [string] $SourceWork, [scriptblock] $Prepare, [scriptblock] $Validate,
        [bool] $StateValid = $false, [bool] $Confirmed = $false, [bool] $NonInteractive = $false,
        [object] $GlobalPlan, [string] $GlobalSource, [string] $GlobalTarget,
        [string] $WorkFailurePoint = '', [string] $GlobalFailurePoint = ''
    )
    $workArguments = @{
        Target = $Target; SourceWork = $SourceWork; Prepare = $Prepare; Validate = $Validate
        StateValid = $StateValid; Confirmed = $Confirmed; NonInteractive = $NonInteractive
        FailurePoint = $WorkFailurePoint
    }
    if ($null -eq $GlobalPlan -or $GlobalPlan.Action -in @('skip', 'unchanged')) {
        return Invoke-InstallerWorkTransaction @workArguments
    }
    $inspection = Inspect-InstallerTarget -Target $Target -SourceWork $SourceWork -StateValid $StateValid
    if (-not (Test-InstallerTargetAuthorization -Inspection $inspection -NonInteractive $NonInteractive -Confirmed $Confirmed)) {
        throw 'Work target is not authorized before Global AGENTS change'
    }
    $parent = Split-Path -Path $Target -Parent
    $globalTransaction = Join-Path $parent ('.agents-install.' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $globalTransaction -ErrorAction Stop | Out-Null
    Write-InstallerGlobalTransactionRecord -Transaction $globalTransaction -WorkTarget $Target -GlobalTarget $GlobalTarget -Plan $GlobalPlan
    try {
        $applied = Invoke-InstallerGlobalAgentsStage -Plan $GlobalPlan -Source $GlobalSource -Target $GlobalTarget -Transaction $globalTransaction -FailurePoint $GlobalFailurePoint
        try {
            $workArguments.DeferCleanup = $true
            $result = Invoke-InstallerWorkTransaction @workArguments
        } catch {
            $workError = $_
            if ($applied) {
                try {
                    Undo-InstallerGlobalAgentsStage -Plan $GlobalPlan -Target $GlobalTarget -Transaction $globalTransaction
                } catch {
                    throw "Work failed and Global AGENTS rollback failed; recovery directory: $globalTransaction. $_"
                }
            }
            throw $workError
        }
        $workTransaction = $script:InstallerLastWorkTransaction
        try { Write-InstallerGlobalWorkVerified -Transaction $globalTransaction -WorkTransaction $workTransaction }
        catch {
            Write-Warning "Work installed; combined transaction awaits verification cleanup: $globalTransaction. $_"
            return $result
        }
        try { Remove-InstallerWorkTransactionSafe -Transaction $workTransaction -SourceWork $SourceWork }
        catch { Write-Warning "Work installed; old transaction awaits cleanup: $workTransaction. $_" }
        try { Remove-InstallerGlobalTransactionSafe -Transaction $globalTransaction }
        catch { Write-Warning "Global AGENTS previous file awaits cleanup: $globalTransaction. $_" }
        return $result
    } catch {
        throw "Combined installation failed; Global AGENTS recovery directory: $globalTransaction. $_"
    }
}
