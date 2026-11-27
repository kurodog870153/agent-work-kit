Set-StrictMode -Version Latest

# One transaction covers Work, Tooling, Shared, the CLI binaries, optional Global Instructions and the install state.
# Nothing is valid until the "committed" marker exists; recovery before the marker restores every previous
# component, recovery after it only finishes the state publication. Layout matches the macOS installer:
# plan (name<TAB>had-previous<TAB>final path), created, staged/<name>, backup/<name>, committed.
function Invoke-InstallerInject {
    param([string] $Point)
    if ($env:INSTALLER_TEST_CRASH -ceq $Point) { exit 97 }
    if ($env:INSTALLER_TEST_FAILURE -ceq $Point) { throw "injected failure at $Point" }
}

function Get-InstallerTxFinal {
    param([string] $Name)
    switch -CaseSensitive ($Name) {
        { $_ -in 'work', 'tooling', 'shared' } { return (Join-Path $script:InstallerSkillsDir $Name) }
        { $_ -in 'host-work', 'host-tooling' } { return (Join-Path $script:InstallerHostSkillsDir "$($Name.Substring(5))/SKILL.md") }
        'bin' { return (Join-Path $script:InstallerCliDir 'bin') }
        'state' { return (Join-Path $script:InstallerInstallDir 'install-state.json') }
        'global' { return $script:InstallerGlobalTarget }
    }
    throw "unknown component $Name"
}

function Test-InstallerExists { param([string] $Path) return (Test-Path -LiteralPath $Path) -or ($null -ne (Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue)) }

function New-InstallerDirectories {
    param([string] $Path)
    $missing = @()
    $probe = $Path
    while (-not (Test-Path -LiteralPath $probe -PathType Container)) {
        if (Test-InstallerExists $probe) { throw "not a directory: $probe" }
        $missing += $probe
        $next = Split-Path -Path $probe -Parent
        if ([string]::IsNullOrEmpty($next) -or $next -eq $probe) { throw 'cannot resolve parent directory' }
        $probe = $next
    }
    for ($index = $missing.Count - 1; $index -ge 0; $index--) {
        New-Item -ItemType Directory -Path $missing[$index] -ErrorAction Stop | Out-Null
        $script:InstallerCreatedDirs += $missing[$index]
    }
}

function Remove-InstallerDirectories {
    param([string[]] $Directories)
    for ($index = $Directories.Count - 1; $index -ge 0; $index--) {
        $path = $Directories[$index]
        if (-not (Test-Path -LiteralPath $path -PathType Container)) { continue }
        if (@(Get-ChildItem -LiteralPath $path -Force).Count -gt 0) { break }
        Remove-Item -LiteralPath $path -ErrorAction Stop
    }
}

function Remove-InstallerTx {
    $tx = $script:InstallerTx
    if (-not (Test-Path -LiteralPath $tx -PathType Container) -or (Test-InstallerReparse (Get-Item -LiteralPath $tx -Force))) { throw 'unsafe installation transaction' }
    foreach ($item in @(Get-ChildItem -LiteralPath $tx -Recurse -Force)) {
        if (Test-InstallerReparse $item) { throw 'unsafe installation transaction' }
    }
    Remove-Item -LiteralPath $tx -Recurse -Force -ErrorAction Stop
}

function Start-InstallerTx {
    $tx = $script:InstallerTx
    New-Item -ItemType Directory -Path $tx -ErrorAction Stop | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $tx 'staged'), (Join-Path $tx 'backup') -ErrorAction Stop | Out-Null
    [IO.File]::WriteAllText((Join-Path $tx 'format'), 'physical-entries-v1', [Text.UTF8Encoding]::new($false))
    Add-InstallerTxCreated -Directories $script:InstallerCreatedDirs -Reset
}

function Add-InstallerTxCreated {
    param([string[]] $Directories, [switch] $Reset)
    $path = Join-Path $script:InstallerTx 'created'
    $text = (@($Directories) | ForEach-Object { "$_`n" }) -join ''
    if ($Reset) { [IO.File]::WriteAllText($path, $text, [Text.UTF8Encoding]::new($false)) }
    else { [IO.File]::AppendAllText($path, $text, [Text.UTF8Encoding]::new($false)) }
}

function Write-InstallerTxPlan {
    param([string[]] $Names)
    $lines = foreach ($name in $Names) {
        $final = Get-InstallerTxFinal $name
        if (($name -clike 'host-*' -or $name -ceq 'global') -and (Test-InstallerExists "$final.agent-work-kit-tmp")) {
            throw "transaction temporary file is occupied: $final"
        }
        $had = if (Test-InstallerExists $final) { 1 } else { 0 }
        "$name`t$had`t$final"
    }
    $tmp = Join-Path $script:InstallerTx 'plan.tmp'
    [IO.File]::WriteAllText($tmp, (($lines -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $tmp -Destination (Join-Path $script:InstallerTx 'plan') -ErrorAction Stop
}

function Move-InstallerComponent {
    param([string] $From, [string] $To)
    Move-Item -LiteralPath $From -Destination $To -ErrorAction Stop
}

# External file participants may be on a different volume. Retain the prepared
# file and intact backup; publish through a same-volume sibling temporary file.
function Assert-InstallerPhysicalFileOrAbsent {
    param([string] $Path)
    if (Test-InstallerExists $Path) {
        $item = Get-Item -LiteralPath $Path -Force
        if ($item.PSIsContainer -or (Test-InstallerReparse $item)) { throw "unsafe transaction file: $Path" }
    }
}

function Copy-InstallerTransactionFile {
    param([string] $From, [string] $To)
    if (-not (Test-InstallerDirectoryPathSafe (Split-Path -Parent $To))) { throw "unsafe transaction destination: $To" }
    Assert-InstallerPhysicalFileOrAbsent $From
    Assert-InstallerPhysicalFileOrAbsent $To
    $tmp = "$To.agent-work-kit-tmp"
    if ((Test-InstallerExists $tmp) -or -not (Test-Path -LiteralPath $From -PathType Leaf)) { throw "unsafe or missing transaction file: $From" }
    [IO.File]::Copy($From, $tmp, $false)
    if ((Get-InstallerFileSha256 $From) -cne (Get-InstallerFileSha256 $tmp)) { throw "transaction copy failed verification: $To" }
    if (Test-InstallerExists $To) {
        [IO.File]::Replace($tmp, $To, [System.Management.Automation.Language.NullString]::Value)
    } else {
        [IO.File]::Move($tmp, $To)
    }
}

function Restore-InstallerTransactionFile {
    param([string] $Final, [string] $Staged, [string] $Backup, [string] $Had)
    if (-not (Test-InstallerDirectoryPathSafe (Split-Path -Parent $Final))) { throw "unsafe recovery destination: $Final" }
    $tmp = "$Final.agent-work-kit-tmp"
    foreach ($file in $Final, $Staged, $Backup, $tmp) { Assert-InstallerPhysicalFileOrAbsent $file }
    if (Test-InstallerExists $Backup) {
        if (Test-InstallerExists $Final) {
            $hash = Get-InstallerFileSha256 $Final
            if ($hash -cne (Get-InstallerFileSha256 $Backup) -and $hash -cne (Get-InstallerFileSha256 $Staged)) { throw "transaction file was modified: $Final" }
        }
        if (Test-InstallerExists $tmp) { [IO.File]::Delete($tmp) }
        Copy-InstallerTransactionFile -From $Backup -To $Final
    } elseif ($Had -ceq '0' -and (Test-InstallerExists $Final)) {
        if ((Get-InstallerFileSha256 $Final) -cne (Get-InstallerFileSha256 $Staged)) { throw "transaction file was modified: $Final" }
        [IO.File]::Delete($Final)
    }
    if (Test-InstallerExists $tmp) { [IO.File]::Delete($tmp) }
}

function Invoke-InstallerTxApply {
    param([string] $Name)
    $final = Get-InstallerTxFinal $Name
    $staged = Join-Path $script:InstallerTx "staged/$Name"
    $backup = Join-Path $script:InstallerTx "backup/$Name"
    if ($Name -clike 'host-*' -or $Name -ceq 'global') {
        if (Test-InstallerExists $final) { Copy-InstallerTransactionFile -From $final -To $backup }
        Invoke-InstallerInject "apply-$Name-moved"
        Copy-InstallerTransactionFile -From $staged -To $final
        Invoke-InstallerInject "apply-$Name"
        return
    }
    if (-not (Test-InstallerExists $staged)) { throw "staged $Name is missing" }
    if (Test-InstallerExists $final) { Move-InstallerComponent -From $final -To $backup }
    Invoke-InstallerInject "apply-$Name-moved"
    Move-InstallerComponent -From $staged -To $final
    Invoke-InstallerInject "apply-$Name"
}

function Restore-InstallerTxComponent {
    param([string] $Name, [string] $Had, [string] $Final)
    $staged = Join-Path $script:InstallerTx "staged/$Name"
    $backup = Join-Path $script:InstallerTx "backup/$Name"
    if ($Name -clike 'host-*' -or $Name -ceq 'global') {
        Restore-InstallerTransactionFile -Final $Final -Staged $staged -Backup $backup -Had $Had
        return
    }
    if (Test-InstallerExists $backup) {
        if (Test-InstallerExists $final) {
            if (Test-InstallerExists $staged) { throw "ambiguous state for $Name" }
            Remove-Item -LiteralPath $final -Recurse -Force -ErrorAction Stop
        }
        Move-InstallerComponent -From $backup -To $final
    } elseif ($Had -ceq '0' -and -not (Test-InstallerExists $staged) -and (Test-InstallerExists $final)) {
        Remove-Item -LiteralPath $final -Recurse -Force -ErrorAction Stop
    }
}

# Loads and validates the recorded plan; returns $null when no plan was recorded, throws when the transaction is unsafe.
function Read-InstallerTxPlan {
    $tx = $script:InstallerTx
    if (-not (Test-Path -LiteralPath $tx -PathType Container) -or (Test-InstallerReparse (Get-Item -LiteralPath $tx -Force))) { throw 'unsafe installation transaction' }
    $format = Join-Path $tx 'format'
    if (-not (Test-Path -LiteralPath $format -PathType Leaf) -or (Test-InstallerReparse (Get-Item -LiteralPath $format -Force)) -or
        [IO.File]::ReadAllText($format) -cne 'physical-entries-v1') { throw 'unsafe or unsupported installation transaction' }
    foreach ($item in @(Get-ChildItem -LiteralPath $tx -Force)) {
        if (Test-InstallerReparse $item) { throw 'unsafe installation transaction' }
        $valid = switch -CaseSensitive ($item.Name) {
            { $_ -in 'staged', 'backup' } { $item.PSIsContainer }
            { $_ -in 'plan', 'plan.tmp', 'created', 'committed', 'format' } { -not $item.PSIsContainer }
            default { $false }
        }
        if (-not $valid) { throw "unexpected entry in installation transaction: $($item.Name)" }
    }
    foreach ($item in @(Get-ChildItem -LiteralPath $tx -Recurse -Force)) {
        if (Test-InstallerReparse $item) { throw 'unsafe installation transaction' }
    }
    $planPath = Join-Path $tx 'plan'
    if (-not (Test-Path -LiteralPath $planPath -PathType Leaf)) { return $null }
    $statePath = Join-Path $tx 'staged/state'
    if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) { $statePath = Join-Path $script:InstallerInstallDir 'install-state.json' }
    $state = Read-InstallerState -Path $statePath
    if ($null -eq $state -or $state.host -cne $script:InstallerSelectedHost -or
        -not (Test-InstallerPathEqual $state.install_dir $script:InstallerInstallDir)) { throw 'invalid transaction state' }
    $globalTarget = if ($null -ne $state.global_instructions_dir) {
        Join-Path $state.global_instructions_dir $(if ($state.host -ceq 'claude') { 'CLAUDE.md' } else { 'AGENTS.md' })
    } else { '' }
    $entries = @()
    $seen = @()
    foreach ($line in [IO.File]::ReadAllText($planPath).TrimEnd("`n").Split("`n")) {
        $parts = $line.Split("`t")
        if ($parts.Count -ne 3 -or $parts[1] -cnotin @('0', '1')) { throw 'invalid installation plan' }
        $name = $parts[0]
        if ($name -cin @('work', 'tooling', 'shared', 'bin', 'state', 'host-work', 'host-tooling')) {

            if (-not (Test-InstallerPathEqual (Get-InstallerTxFinal $name) $parts[2])) { throw 'installation plan targets another location' }
        } elseif ($name -ceq 'global') {
            if (-not $globalTarget -or -not (Test-InstallerPathEqual $globalTarget $parts[2])) { throw 'invalid installation plan' }
        } else { throw 'invalid installation plan' }
        if ($seen -ccontains $name -or -not (Test-InstallerDirectoryPathSafe (Split-Path -Parent $parts[2]))) { throw 'unsafe or duplicate installation participant' }
        $seen += $name
        $entries += , @($name, $parts[1], $parts[2])
    }
    foreach ($required in 'work', 'tooling', 'shared', 'bin', 'host-work', 'host-tooling', 'state') {
        if ($seen -cnotcontains $required) { throw 'incomplete installation plan' }
    }
    return , $entries
}

function Complete-InstallerTxRollback {
    $created = @()
    $path = Join-Path $script:InstallerTx 'created'
    if (Test-Path -LiteralPath $path -PathType Leaf) {
        $created = @([IO.File]::ReadAllText($path).Split("`n") | Where-Object { $_ -and [IO.Path]::IsPathRooted($_) })
    }
    Remove-InstallerTx
    Remove-InstallerDirectories -Directories $created
}

function Undo-InstallerTx {
    $plan = Read-InstallerTxPlan
    if ($null -ne $plan) {
        for ($index = $plan.Count - 1; $index -ge 0; $index--) {
            Restore-InstallerTxComponent -Name $plan[$index][0] -Had $plan[$index][1] -Final $plan[$index][2]
        }
    }
    Complete-InstallerTxRollback
}

function Complete-InstallerTx {
    $plan = Read-InstallerTxPlan
    if ($null -eq $plan) { throw 'committed installation transaction has no plan' }
    if (Test-InstallerExists (Join-Path $script:InstallerTx 'staged/state')) { Invoke-InstallerTxApply -Name 'state' }
    $state = Read-InstallerState -Path (Join-Path $script:InstallerInstallDir 'install-state.json')
    if ($null -eq $state -or -not (Test-InstallerManifestMatches -InstallDir $script:InstallerInstallDir -Entries @(ConvertTo-InstallerManifestLines -Files $state.files))) { throw 'published installation is invalid' }
    try { Remove-InstallerTx } catch { [Console]::Error.WriteLine("Warning: installation transaction awaits cleanup: $($script:InstallerTx)") }
}

# Called at the start of a run: completes or undoes an interrupted transaction.
function Restore-InstallerInterrupted {
    if (-not (Test-InstallerExists $script:InstallerTx)) { return }
    if (Test-Path -LiteralPath (Join-Path $script:InstallerTx 'committed') -PathType Leaf) { Complete-InstallerTx } else { Undo-InstallerTx }
}
