param([string] $Binary)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
if ([string]::IsNullOrEmpty($Binary)) { $Binary = Join-Path $repoRoot 'rust/target/release/work.exe' }
$binary = (Resolve-Path -LiteralPath $Binary).Path
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'native Work binary is required for core fixture' }
$scratch = Join-Path $repoRoot ('.issue95-core.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolved = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolved.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolved) -match '^\.issue95-core\.[0-9a-f]{32}$')) { throw 'unsafe cleanup path' }
$originalUserProfile = $env:USERPROFILE
$originalHome = $env:HOME
$originalPath = $env:PATH
$originalTestWorkBinary = $env:TEST_WORK_BINARY
$testHome = Join-Path $scratch 'home'
New-Item -ItemType Directory -Path $testHome | Out-Null
$env:USERPROFILE = $testHome
$env:HOME = $testHome
function Test-CoreRejected {
    param([string] $Core, [string] $Root, [string] $Target)
    $ErrorActionPreference = 'Continue'
    & powershell.exe -NoProfile -File $Core -Mode package -Root $Root --non-interactive --no-global-agents --skills general --work-install-dir $Target 2>$null | Out-Null
    return $LASTEXITCODE -ne 0
}
function Invoke-CombinedRecoveryExpectedFailure {
    param([string] $Core, [string] $Root, [string] $Global, [string] $Work)
    $ErrorActionPreference = 'Continue'
    @('y', 'y') | & powershell.exe -NoProfile -File $Core -Mode package -Root $Root --global-agents --global-agents-install-dir $Global --skills general --work-install-dir $Work 2>$null | Out-Null
    return $LASTEXITCODE
}
try {
    $linkedCodex = Join-Path $scratch 'linked-codex'
    New-Item -ItemType Directory -Path $linkedCodex | Out-Null
    New-Item -ItemType Junction -Path (Join-Path $testHome '.codex') -Target $linkedCodex | Out-Null
    $root = Join-Path $scratch 'package'
    New-Item -ItemType Directory -Path (Join-Path $root 'skills') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'skills/work') -Destination (Join-Path $root 'skills/work') -Recurse
    New-Item -ItemType Directory -Path (Join-Path $root 'bin') -Force | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $root 'bin/work.exe')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'AGENTS.md') -Destination (Join-Path $root 'AGENTS.md')
    [IO.File]::WriteAllText((Join-Path $root 'release-info.json'), '{"version":"1.0.5"}')
    New-Item -ItemType Directory -Path (Join-Path $root 'installer/state') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/common/installer/install-state.json') -Destination (Join-Path $root 'installer/state/install-state.json')
    $target = Join-Path $scratch 'installed/work'
    $core = Join-Path $repoRoot 'os-scripts/windows/installer/runtime/install.ps1'
    $sourceEntry = Join-Path $repoRoot 'os-scripts/windows/install-work-from-source.bat'
    $specialDirectory = ([string]::Concat([char]0x76EE, [char]0x9304)) + ' & ' +
        ([string]::Concat([char]0x7A7A, [char]0x767D))
    $specialTarget = Join-Path (Join-Path $scratch $specialDirectory) 'work'
    & $sourceEntry --non-interactive --no-global-agents --skills general --work-install-dir $specialTarget --dry-run | Out-Null
    if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $specialTarget)) { throw 'Source batch argument boundary failed' }
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --no-global-agents --skills general --work-install-dir $target --dry-run | Out-Null
    if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $target)) { throw 'dry run modified target or failed' }
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --no-global-agents --skills general --work-install-dir $target | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'package core first install failed' }
    if (-not (Test-Path -LiteralPath (Join-Path $target 'scripts/work.exe')) -or
        -not (Test-Path -LiteralPath (Join-Path $target 'install-state.json'))) { throw 'installed files missing' }
    $state = Get-Content -LiteralPath (Join-Path $target 'install-state.json') -Raw | ConvertFrom-Json
    if ($state.version -ne '1.0.5' -or $state.work_dir -ne $target -or
        $null -ne $state.global_agents_dir -or $state.skills.Count -ne 1 -or $state.skills[0] -ne 'general') {
        throw 'installed state is wrong'
    }
    if (Test-Path -LiteralPath (Join-Path $linkedCodex 'AGENTS.md')) {
        throw '--no-global-agents modified linked Global AGENTS directory'
    }
    [IO.File]::WriteAllText((Join-Path $target 'unknown.txt'), 'keep')
    foreach ($mode in @('interactive', 'non-interactive')) {
        $ErrorActionPreference = 'Continue'
        if ($mode -eq 'non-interactive') {
            $message = & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --no-global-agents --skills general --work-install-dir $target 2>&1 | Out-String
        } else {
            $message = @('y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --no-global-agents --skills general --work-install-dir $target 2>&1 | Out-String
        }
        $failureCode = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if ($failureCode -eq 0 -or $message -notmatch 'unknown entry: unknown.txt' -or
            $message -notmatch 'work-install-dir' -or [IO.File]::ReadAllText((Join-Path $target 'unknown.txt')) -cne 'keep') {
            throw "$mode unsafe Work diagnostic or preservation failed (exit $failureCode): $message"
        }
    }
    Remove-Item -LiteralPath (Join-Path $target 'unknown.txt')
    $ErrorActionPreference = 'Continue'
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --global-agents --skills general --work-install-dir (Join-Path $scratch 'unsafe-global/work') 2>$null | Out-Null
    $unsafeGlobalExit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($unsafeGlobalExit -eq 0) { throw 'enabled Global AGENTS accepted linked directory' }
    $interactiveTarget = Join-Path (Join-Path $scratch 'custom Work') 'work'
    @('2', $interactiveTarget, 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --skills general --no-global-agents | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $interactiveTarget 'scripts/work.exe'))) {
        throw 'interactive custom Work directory install failed'
    }
    $globalWorkTarget = Join-Path (Join-Path $scratch 'global Work') 'work'
    $globalDirectory = Join-Path $scratch 'custom Global'
    @('2', $globalDirectory, 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --work-install-dir $globalWorkTarget --skills general --global-agents | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $globalWorkTarget 'scripts/work.exe')) -or
        -not (Test-Path -LiteralPath (Join-Path $globalDirectory 'AGENTS.md'))) {
        throw 'interactive custom Global AGENTS directory install failed'
    }
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --no-global-agents --skills general --work-install-dir $target | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'same-version reinstall failed' }
    [IO.File]::WriteAllText((Join-Path $root 'release-info.json'), '{"version":"1.0.4"}')
    if (-not (Test-CoreRejected -Core $core -Root $root -Target $target)) { throw 'downgrade was accepted' }
    [IO.File]::WriteAllText((Join-Path $root 'release-info.json'), '{"version":"1.0.5"}')
    [IO.File]::WriteAllText((Join-Path $target 'unknown.txt'), 'keep')
    if (-not (Test-CoreRejected -Core $core -Root $root -Target $target) -or [IO.File]::ReadAllText((Join-Path $target 'unknown.txt')) -cne 'keep') { throw 'unknown content was changed' }
    Remove-Item -LiteralPath (Join-Path $target 'unknown.txt')
    $originalBinaryHash = (Get-FileHash -LiteralPath (Join-Path $target 'scripts/work.exe') -Algorithm SHA256).Hash
    Remove-Item -LiteralPath (Join-Path $target 'install-state.json')
    if (-not (Test-CoreRejected -Core $core -Root $root -Target $target) -or (Get-FileHash -LiteralPath (Join-Path $target 'scripts/work.exe') -Algorithm SHA256).Hash -ne $originalBinaryHash) {
        throw 'noninteractive first update changed Work'
    }
    @('YES', 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --no-global-agents --skills general --work-install-dir $target | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $target 'install-state.json'))) {
        throw 'confirmed first update failed'
    }
    $lockedBinary = [IO.File]::Open((Join-Path $target 'scripts/work.exe'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    try {
        if (-not (Test-CoreRejected -Core $core -Root $root -Target $target)) { throw 'in-use Work binary was accepted' }
    } finally { $lockedBinary.Dispose() }
    $defaultTarget = Join-Path $testHome '.agents/skills/work'
    New-Item -ItemType Directory -Path (Split-Path -Path $defaultTarget -Parent) -Force | Out-Null
    Copy-Item -LiteralPath $target -Destination $defaultTarget -Recurse
    $oldState = Get-Content -LiteralPath (Join-Path $defaultTarget 'install-state.json') -Raw | ConvertFrom-Json
    $oldState.work_dir = $defaultTarget
    [IO.File]::WriteAllText((Join-Path $defaultTarget 'install-state.json'), (($oldState | ConvertTo-Json -Depth 4) + "`n"))
    $migrated = Join-Path $scratch 'migrated/work'
    $lockedOld = [IO.File]::Open((Join-Path $defaultTarget 'scripts/work.exe'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --work-install-dir $migrated | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'default Work migration failed' }
    } finally { $lockedOld.Dispose() }
    $record = Join-Path (Split-Path -Path $migrated -Parent) '.work-migration-cleanup'
    if (-not (Test-Path -LiteralPath $record) -or -not (Test-Path -LiteralPath $defaultTarget)) {
        throw 'locked previous Work was not recorded for cleanup'
    }
    $oldSkill = [IO.File]::ReadAllText((Join-Path $defaultTarget 'SKILL.md'))
    [IO.File]::WriteAllText((Join-Path $defaultTarget 'SKILL.md'), 'changed')
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --work-install-dir $migrated | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $record) -or -not (Test-Path -LiteralPath $defaultTarget)) {
        throw 'changed previous Work was removed'
    }
    [IO.File]::WriteAllText((Join-Path $defaultTarget 'SKILL.md'), $oldSkill)
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --work-install-dir $migrated | Out-Null
    if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $record) -or (Test-Path -LiteralPath $defaultTarget)) {
        throw 'pending migration cleanup was not retried'
    }
    $newState = Get-Content -LiteralPath (Join-Path $migrated 'install-state.json') -Raw | ConvertFrom-Json
    if ($newState.work_dir -ne $migrated -or
        $null -ne $newState.global_agents_dir) { throw 'migration did not preserve settings' }
    $crashInstaller = Join-Path $scratch 'crash-installer'
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/windows/installer') -Destination $crashInstaller -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/common/installer/options.tsv') -Destination (Join-Path $crashInstaller 'cli/options.tsv')
    $transactionSource = Join-Path $repoRoot 'os-scripts/windows/installer/transaction/work.ps1'
    $crashCore = Join-Path $crashInstaller 'runtime/install.ps1'
    foreach ($point in @('staging', 'first-rename', 'second-rename', 'before-verified', 'after-verified')) {
        $crashTarget = Join-Path (Join-Path $scratch "crash-$point") 'work'
        $crashGlobal = Join-Path (Join-Path $scratch "crash-$point") 'global'
        [IO.File]::WriteAllText((Join-Path $root 'release-info.json'), '{"version":"1.0.5"}')
        & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --global-agents --global-agents-install-dir $crashGlobal --skills general --work-install-dir $crashTarget | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "initial $point Work installation failed" }
        $globalBefore = (Get-FileHash -LiteralPath (Join-Path $crashGlobal 'AGENTS.md') -Algorithm SHA256).Hash
        [IO.File]::WriteAllText((Join-Path $root 'release-info.json'), '{"version":"1.0.6"}')
        $contents = [IO.File]::ReadAllText($transactionSource)
        $kill = '[Diagnostics.Process]::GetCurrentProcess().Kill()'
        switch ($point) {
            'staging' { $needle = "        `$inspection = Inspect-InstallerTarget -Target `$targetPath -SourceWork `$SourceWork -StateValid `$StateValid" }
            'first-rename' { $needle = '            Move-Item -LiteralPath $targetPath -Destination $previous -ErrorAction Stop' }
            'second-rename' { $needle = '            Move-Item -LiteralPath $prepared -Destination $targetPath -ErrorAction Stop' }
            'before-verified' { $needle = "        [IO.File]::WriteAllText((Join-Path `$transaction 'verified.txt'), 'verified')" }
            'after-verified' { $needle = "        [IO.File]::WriteAllText((Join-Path `$transaction 'verified.txt'), 'verified')" }
        }
        if ($contents.Split([string[]]@($needle), [StringSplitOptions]::None).Count -ne 2) {
            throw "ambiguous $point crash injection point"
        }
        if ($point -eq 'before-verified') { $contents = $contents.Replace($needle, "        $kill`n$needle") }
        else { $contents = $contents.Replace($needle, "$needle`n        $kill") }
        [IO.File]::WriteAllText((Join-Path $crashInstaller 'transaction/work.ps1'), $contents)
        & powershell.exe -NoProfile -File $crashCore -Mode package -Root $root --non-interactive --global-agents --global-agents-install-dir $crashGlobal --skills general --work-install-dir $crashTarget 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) { throw "$point crash simulation completed successfully" }
        $transaction = @(Get-ChildItem -LiteralPath (Split-Path -Path $crashTarget -Parent) -Directory -Force |
            Where-Object { $_.Name -match '^\.work-install\.[0-9a-f]{32}$' })
        if ($transaction.Count -ne 1) { throw "$point crash did not leave one recovery record" }
        $oldPath = if (Test-Path -LiteralPath (Join-Path $transaction[0].FullName 'previous')) {
            Join-Path $transaction[0].FullName 'previous'
        } else { $crashTarget }
        $oldState = Get-Content -LiteralPath (Join-Path $oldPath 'install-state.json') -Raw | ConvertFrom-Json
        if ($oldState.version -ne '1.0.5' -or
            (Get-FileHash -LiteralPath (Join-Path $crashGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $globalBefore) {
            throw "$point crash lost previous Work or changed Global AGENTS"
        }
        & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --global-agents --global-agents-install-dir $crashGlobal --skills general --work-install-dir $crashTarget | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "$point crash recovery failed" }
        $crashState = Get-Content -LiteralPath (Join-Path $crashTarget 'install-state.json') -Raw | ConvertFrom-Json
        if ($crashState.version -ne '1.0.6' -or -not (Test-Path -LiteralPath (Join-Path $crashTarget 'scripts/work.exe')) -or
            (Test-Path -LiteralPath $transaction[0].FullName) -or
            (Get-FileHash -LiteralPath (Join-Path $crashGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $globalBefore) {
            throw "$point did not finish new Work installation or changed Global AGENTS"
        }
    }
    $originalGlobalSource = [IO.File]::ReadAllBytes((Join-Path $root 'AGENTS.md'))
    foreach ($point in @('after-global', 'during-work', 'after-work-verified')) {
        $caseRoot = Join-Path $scratch "global-crash-$point"
        $caseWork = Join-Path $caseRoot 'work'
        $caseGlobal = Join-Path $caseRoot 'global'
        [IO.File]::WriteAllBytes((Join-Path $root 'AGENTS.md'), $originalGlobalSource)
        [IO.File]::WriteAllText((Join-Path $root 'release-info.json'), '{"version":"1.0.5"}')
        & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --global-agents --global-agents-install-dir $caseGlobal --skills general --work-install-dir $caseWork | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "$point initial combined installation failed" }
        $oldGlobalHash = (Get-FileHash -LiteralPath (Join-Path $caseGlobal 'AGENTS.md') -Algorithm SHA256).Hash
        [IO.File]::WriteAllText((Join-Path $root 'AGENTS.md'), "new Global AGENTS for $point`n")
        $newGlobalHash = (Get-FileHash -LiteralPath (Join-Path $root 'AGENTS.md') -Algorithm SHA256).Hash
        [IO.File]::WriteAllText((Join-Path $root 'release-info.json'), '{"version":"1.0.6"}')
        $contents = [IO.File]::ReadAllText($transactionSource)
        $kill = '[Diagnostics.Process]::GetCurrentProcess().Kill()'
        switch ($point) {
            'after-global' { $needle = '        $applied = Invoke-InstallerGlobalAgentsStage -Plan $GlobalPlan -Source $GlobalSource -Target $GlobalTarget -Transaction $globalTransaction -FailurePoint $GlobalFailurePoint' }
            'during-work' { $needle = "            (Get-InstallerWorkFileSha256 (Join-Path `$prepared 'scripts/work.exe')))" }
            'after-work-verified' { $needle = "        [IO.File]::WriteAllText((Join-Path `$transaction 'verified.txt'), 'verified')" }
        }
        if ($contents.Split([string[]]@($needle), [StringSplitOptions]::None).Count -ne 2) {
            throw "ambiguous $point combined crash injection point"
        }
        [IO.File]::WriteAllText((Join-Path $crashInstaller 'transaction/work.ps1'),
            $contents.Replace($needle, "$needle`n        $kill"))
        @('y', 'y') | & powershell.exe -NoProfile -File $crashCore -Mode package -Root $root --global-agents --global-agents-install-dir $caseGlobal --skills general --work-install-dir $caseWork 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0 -or
            (Get-FileHash -LiteralPath (Join-Path $caseGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $newGlobalHash) {
            throw "$point did not interrupt after Global AGENTS replacement"
        }
        $globalTransaction = @(Get-ChildItem -LiteralPath $caseRoot -Directory -Force |
            Where-Object { $_.Name -match '^\.agents-install\.[0-9a-f]{32}$' })
        if ($globalTransaction.Count -ne 1 -or
            (Get-FileHash -LiteralPath (Join-Path $globalTransaction[0].FullName 'global-previous') -Algorithm SHA256).Hash -ne $oldGlobalHash) {
            throw "$point lost previous Global AGENTS"
        }
        if ($point -eq 'after-global') {
            $recordPath = Join-Path $globalTransaction[0].FullName 'record.json'
            $originalRecord = [IO.File]::ReadAllBytes($recordPath)
            [IO.File]::WriteAllText($recordPath, '{"source_sha256":"invalid"}')
            $failureCode = Invoke-CombinedRecoveryExpectedFailure -Core $core -Root $root -Global $caseGlobal -Work $caseWork
            if ($failureCode -eq 0 -or
                (Get-FileHash -LiteralPath (Join-Path $caseGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $newGlobalHash -or
                (Get-FileHash -LiteralPath (Join-Path $globalTransaction[0].FullName 'global-previous') -Algorithm SHA256).Hash -ne $oldGlobalHash) {
                throw 'damaged Global transaction record changed installed or previous content'
            }
            [IO.File]::WriteAllBytes($recordPath, $originalRecord)
            [IO.File]::WriteAllText((Join-Path $caseGlobal 'AGENTS.md'), 'external edit')
            $failureCode = Invoke-CombinedRecoveryExpectedFailure -Core $core -Root $root -Global $caseGlobal -Work $caseWork
            if ($failureCode -eq 0 -or
                [IO.File]::ReadAllText((Join-Path $caseGlobal 'AGENTS.md')) -cne 'external edit' -or
                (Get-FileHash -LiteralPath (Join-Path $globalTransaction[0].FullName 'global-previous') -Algorithm SHA256).Hash -ne $oldGlobalHash) {
                throw 'external Global AGENTS edit was overwritten or previous content was lost'
            }
            Copy-Item -LiteralPath (Join-Path $root 'AGENTS.md') -Destination (Join-Path $caseGlobal 'AGENTS.md') -Force
            $unknownPath = Join-Path $globalTransaction[0].FullName 'unknown.txt'
            [IO.File]::WriteAllText($unknownPath, 'keep')
            $failureCode = Invoke-CombinedRecoveryExpectedFailure -Core $core -Root $root -Global $caseGlobal -Work $caseWork
            if ($failureCode -eq 0 -or [IO.File]::ReadAllText($unknownPath) -cne 'keep' -or
                (Get-FileHash -LiteralPath (Join-Path $globalTransaction[0].FullName 'global-previous') -Algorithm SHA256).Hash -ne $oldGlobalHash) {
                throw 'unknown Global transaction content was changed or accepted'
            }
            Remove-Item -LiteralPath $unknownPath
        }
        $restartOutput = @('y', 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --global-agents --global-agents-install-dir $caseGlobal --skills general --work-install-dir $caseWork
        if ($LASTEXITCODE -ne 0) { throw "$point combined recovery failed" }
        if ($point -ne 'after-work-verified' -and ($restartOutput -join "`n") -notmatch 'Global AGENTS.md differs') {
            throw "$point did not restore previous Global AGENTS before reinstall"
        }
        $finalState = Get-Content -LiteralPath (Join-Path $caseWork 'install-state.json') -Raw | ConvertFrom-Json
        $remaining = @(Get-ChildItem -LiteralPath $caseRoot -Directory -Force |
            Where-Object { $_.Name -match '^\.(agents|work)-install\.' })
        if ($finalState.version -ne '1.0.6' -or
            (Get-FileHash -LiteralPath (Join-Path $caseGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $newGlobalHash -or
            $remaining.Count -ne 0) { throw "$point left an incomplete combined transaction" }
    }
    $sourceProject = Join-Path $scratch 'source-project'
    New-Item -ItemType Directory -Path (Join-Path $sourceProject 'os-scripts') -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $sourceProject 'skills') -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $sourceProject 'rust') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/windows') -Destination (Join-Path $sourceProject 'os-scripts/windows') -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'skills/work') -Destination (Join-Path $sourceProject 'skills/work') -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/common/installer/options.tsv') -Destination (Join-Path $sourceProject 'os-scripts/windows/installer/cli/options.tsv')
    New-Item -ItemType Directory -Path (Join-Path $sourceProject 'os-scripts/common/installer') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/common/installer/install-state.json') -Destination (Join-Path $sourceProject 'os-scripts/common/installer/install-state.json')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'AGENTS.md') -Destination (Join-Path $sourceProject 'AGENTS.md')
    [IO.File]::WriteAllText((Join-Path $sourceProject 'VERSION'), "1.0.5`n")
    $cargoShim = Join-Path $scratch 'cargo-shim'
    New-Item -ItemType Directory -Path $cargoShim | Out-Null
    [IO.File]::WriteAllText((Join-Path $cargoShim 'cargo.cmd'),
        "@echo off`r`nif not exist `"%TEST_WORK_BINARY%`" (echo test Work binary is missing 1>&2 & exit /b 1)`r`nif not exist target\release mkdir target\release`r`ncopy /Y `"%TEST_WORK_BINARY%`" target\release\work.exe >nul`r`n")
    $env:TEST_WORK_BINARY = $binary
    $env:PATH = "$cargoShim;$originalPath"
    $publicSourceEntry = Join-Path $sourceProject 'os-scripts/windows/install-work-from-source.bat'
    $publicWork = Join-Path $scratch 'public-source/work'
    $publicGlobal = Join-Path $scratch 'public-source/global'
    & $publicSourceEntry --non-interactive --global-agents --global-agents-install-dir $publicGlobal --skills general --work-install-dir $publicWork | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'public Source initial installation failed' }
    $oldPublicGlobalHash = (Get-FileHash -LiteralPath (Join-Path $publicGlobal 'AGENTS.md') -Algorithm SHA256).Hash
    [IO.File]::WriteAllText((Join-Path $sourceProject 'VERSION'), "1.0.6`n")
    [IO.File]::WriteAllText((Join-Path $sourceProject 'AGENTS.md'), "new public Source Global AGENTS`n")
    $newPublicGlobalHash = (Get-FileHash -LiteralPath (Join-Path $sourceProject 'AGENTS.md') -Algorithm SHA256).Hash
    $publicTransaction = Join-Path $sourceProject 'os-scripts/windows/installer/transaction/work.ps1'
    $publicContents = [IO.File]::ReadAllText($publicTransaction)
    $publicMarker = '        $applied = Invoke-InstallerGlobalAgentsStage -Plan $GlobalPlan -Source $GlobalSource -Target $GlobalTarget -Transaction $globalTransaction -FailurePoint $GlobalFailurePoint'
    if ($publicContents.Split([string[]]@($publicMarker), [StringSplitOptions]::None).Count -ne 2) {
        throw 'public Source crash injection point is ambiguous'
    }
    [IO.File]::WriteAllText($publicTransaction, $publicContents.Replace($publicMarker,
        "$publicMarker`n        [Diagnostics.Process]::GetCurrentProcess().Kill()"))
    $ErrorActionPreference = 'Continue'
    @('y', 'y') | & $publicSourceEntry --global-agents --global-agents-install-dir $publicGlobal --skills general --work-install-dir $publicWork 2>$null | Out-Null
    $publicCrashExit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    $publicTransactions = @(Get-ChildItem -LiteralPath (Split-Path -Path $publicWork -Parent) -Directory -Force |
        Where-Object { $_.Name -match '^\.agents-install\.[0-9a-f]{32}$' })
    $oldPublicState = Get-Content -LiteralPath (Join-Path $publicWork 'install-state.json') -Raw | ConvertFrom-Json
    if ($publicCrashExit -eq 0 -or $publicTransactions.Count -ne 1 -or $oldPublicState.version -ne '1.0.5' -or
        (Get-FileHash -LiteralPath (Join-Path $publicTransactions[0].FullName 'global-previous') -Algorithm SHA256).Hash -ne $oldPublicGlobalHash -or
        (Get-FileHash -LiteralPath (Join-Path $publicGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $newPublicGlobalHash) {
        throw 'public Source interruption lost the previous install'
    }
    [IO.File]::WriteAllText($publicTransaction, $publicContents)
    @('y', 'y') | & $publicSourceEntry --global-agents --global-agents-install-dir $publicGlobal --skills general --work-install-dir $publicWork | Out-Null
    $publicState = Get-Content -LiteralPath (Join-Path $publicWork 'install-state.json') -Raw | ConvertFrom-Json
    $remainingPublic = @(Get-ChildItem -LiteralPath (Split-Path -Path $publicWork -Parent) -Directory -Force |
        Where-Object { $_.Name -match '^\.(agents|work)-install\.' })
    if ($LASTEXITCODE -ne 0 -or $publicState.version -ne '1.0.6' -or
        (Get-FileHash -LiteralPath (Join-Path $publicGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $newPublicGlobalHash -or
        $remainingPublic.Count -ne 0) { throw 'public Source restart did not finish the transaction' }
    Write-Output 'Windows installer core fixture passed.'
} finally {
    $env:USERPROFILE = $originalUserProfile
    $env:HOME = $originalHome
    $env:PATH = $originalPath
    $env:TEST_WORK_BINARY = $originalTestWorkBinary
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
