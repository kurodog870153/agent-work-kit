param([string] $Binary)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
$baseVersion = ([IO.File]::ReadAllText((Join-Path $repoRoot 'VERSION'))).Trim()
$parts = [version] $baseVersion
$nextVersion = "$($parts.Major).$($parts.Minor).$($parts.Build + 1)"
$olderVersion = if ($parts.Build -gt 0) { "$($parts.Major).$($parts.Minor).$($parts.Build - 1)" }
    elseif ($parts.Minor -gt 0) { "$($parts.Major).$($parts.Minor - 1).0" }
    else { "$($parts.Major - 1).0.0" }
$workspaceManifest = [IO.File]::ReadAllText((Join-Path $repoRoot 'rust/Cargo.toml'))
$workspaceVersion = [regex]::Match($workspaceManifest, '(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"$').Groups[1].Value
if ([string]::IsNullOrEmpty($workspaceVersion)) { throw 'Rust workspace test version is missing' }
function Write-TestReleaseInfo {
    param([string] $Root, [string] $Version)
    [IO.File]::WriteAllText((Join-Path $Root 'release-info.json'),
        ([ordered]@{ version = $Version; work_version = $workspaceVersion; tooling_version = $workspaceVersion } | ConvertTo-Json -Compress))
}
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
$originalTestToolingBinary = $env:TEST_TOOLING_BINARY
$testHome = Join-Path $scratch 'home'
New-Item -ItemType Directory -Path $testHome | Out-Null
$env:USERPROFILE = $testHome
$env:HOME = $testHome
function Test-CoreRejected {
    param([string] $Core, [string] $Root, [string] $Target)
    $ErrorActionPreference = 'Continue'
    & powershell.exe -NoProfile -File $Core -Mode package -Root $Root --non-interactive --no-global-agent --skills-install-dir (Split-Path -Parent $Target) 2>$null | Out-Null
    return $LASTEXITCODE -ne 0
}
function Invoke-CombinedRecoveryExpectedFailure {
    param([string] $Core, [string] $Root, [string] $Global, [string] $Work)
    $ErrorActionPreference = 'Continue'
    @('y', 'y') | & powershell.exe -NoProfile -File $Core -Mode package -Root $Root --global-agent --global-agent-install-dir $Global --skills-install-dir (Split-Path -Parent $Work) 2>$null | Out-Null
    return $LASTEXITCODE
}
try {
    $linkedCodex = Join-Path $scratch 'linked-codex'
    New-Item -ItemType Directory -Path $linkedCodex | Out-Null
    New-Item -ItemType Junction -Path (Join-Path $testHome '.codex') -Target $linkedCodex | Out-Null
    $root = Join-Path $scratch 'package'
    New-Item -ItemType Directory -Path (Join-Path $root 'skills') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'skills/work') -Destination (Join-Path $root 'skills/work') -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'skills/tooling') -Destination (Join-Path $root 'skills/tooling') -Recurse
    New-Item -ItemType Directory -Path (Join-Path $root 'bin') -Force | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $root 'bin/work.exe')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'rust/target/release/tooling.exe') -Destination (Join-Path $root 'bin/tooling.exe')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'AGENTS.md') -Destination (Join-Path $root 'AGENTS.md')
    Write-TestReleaseInfo -Root $root -Version $baseVersion
    New-Item -ItemType Directory -Path (Join-Path $root 'installer/state') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/common/installer/install-state.json') -Destination (Join-Path $root 'installer/state/install-state.json')
    $target = Join-Path $scratch 'installed/work'
    $core = Join-Path $repoRoot 'os-scripts/windows/installer/install/install-core.ps1'
    $sourceEntry = Join-Path $repoRoot 'os-scripts/windows/install-work-from-source.bat'
    $specialDirectory = ([string]::Concat([char]0x76EE, [char]0x9304)) + ' & ' +
        ([string]::Concat([char]0x7A7A, [char]0x767D))
    $specialTarget = Join-Path (Join-Path $scratch $specialDirectory) 'work'
    & $sourceEntry --non-interactive --no-global-agent --skills-install-dir (Split-Path -Parent $specialTarget) --dry-run | Out-Null
    if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $specialTarget)) { throw 'Source batch argument boundary failed' }
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --no-global-agent --skills-install-dir (Split-Path -Parent $target) --dry-run | Out-Null
    if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $target)) { throw 'dry run modified target or failed' }
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --no-global-agent --skills-install-dir (Split-Path -Parent $target) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'package core first install failed' }
    $toolingTarget = Join-Path (Split-Path -Parent $target) 'tooling'
    $parentState = Join-Path (Split-Path -Parent $target) 'install-state.json'
    if (-not (Test-Path -LiteralPath (Join-Path $target 'scripts/work.exe')) -or
        -not (Test-Path -LiteralPath $parentState) -or
        (Test-Path -LiteralPath (Join-Path $target 'install-state.json')) -or
        (Test-Path -LiteralPath (Join-Path $toolingTarget 'install-state.json')) -or
        -not (Test-Path -LiteralPath (Join-Path $toolingTarget 'scripts/tooling.exe')) -or
        -not (Test-Path -LiteralPath (Join-Path $toolingTarget 'references/instructions/interfaces/mcp/instructions.md')) -or
        (Test-Path -LiteralPath (Join-Path $toolingTarget 'references/instructions/interfaces/repl'))) { throw 'installed files missing or removed REPL retained' }
    $state = Get-Content -LiteralPath $parentState -Raw | ConvertFrom-Json
    if ($state.version -ne $baseVersion -or $state.work_version -ne $workspaceVersion -or
        $state.tooling_version -ne $workspaceVersion -or
        $state.skill_install_dir -ne (Split-Path -Parent $target) -or
        $null -ne $state.global_agent_dir -or $state.status -ne 'installed' -or
        @($state.PSObject.Properties).Count -ne 6) {
        throw 'installed state is wrong'
    }
    $legacyChildState = Join-Path $target 'install-state.json'
    [IO.File]::WriteAllText($legacyChildState, 'legacy state')
    if (-not (Test-CoreRejected -Core $core -Root $root -Target $target) -or
        [IO.File]::ReadAllText($legacyChildState) -cne 'legacy state') {
        throw 'legacy Work child state was accepted or changed'
    }
    Remove-Item -LiteralPath $legacyChildState
    $toolingMarker = Join-Path $toolingTarget 'unknown.txt'
    [IO.File]::WriteAllText($toolingMarker, 'keep')
    if (-not (Test-CoreRejected -Core $core -Root $root -Target $target) -or
        [IO.File]::ReadAllText($toolingMarker) -cne 'keep') { throw 'unknown Tooling content was accepted or changed' }
    Remove-Item -LiteralPath $toolingMarker
    $legacyToolingParent = Join-Path $scratch 'legacy-tooling'
    New-Item -ItemType Directory -Path $legacyToolingParent | Out-Null
    Copy-Item -LiteralPath $toolingTarget -Destination (Join-Path $legacyToolingParent 'tooling') -Recurse
    $legacyToolingState = Join-Path $legacyToolingParent 'tooling/install-state.json'
    [IO.File]::WriteAllText($legacyToolingState, 'legacy state')
    if (-not (Test-CoreRejected -Core $core -Root $root -Target (Join-Path $legacyToolingParent 'work'))) {
        throw 'noninteractive Tooling takeover was accepted'
    }
    @('YES', 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --no-global-agent --skills-install-dir $legacyToolingParent | Out-Null
    if ($LASTEXITCODE -eq 0 -or
        (Test-Path -LiteralPath (Join-Path $legacyToolingParent 'install-state.json')) -or
        (Test-Path -LiteralPath (Join-Path $legacyToolingParent 'work/scripts/work.exe')) -or
        [IO.File]::ReadAllText($legacyToolingState) -cne 'legacy state') {
        throw 'legacy Tooling directory was taken over'
    }
    if (Test-Path -LiteralPath (Join-Path $linkedCodex 'AGENTS.md')) {
        throw '--no-global-agent modified linked Global AGENTS directory'
    }
    [IO.File]::WriteAllText((Join-Path $target 'unknown.txt'), 'keep')
    foreach ($mode in @('interactive', 'non-interactive')) {
        $ErrorActionPreference = 'Continue'
        if ($mode -eq 'non-interactive') {
            $message = & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --no-global-agent --skills-install-dir (Split-Path -Parent $target) 2>&1 | Out-String
        } else {
            $message = @('y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --no-global-agent --skills-install-dir (Split-Path -Parent $target) 2>&1 | Out-String
        }
        $failureCode = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if ($failureCode -eq 0 -or $message -notmatch 'unknown entry: unknown.txt' -or
            $message -notmatch '--skills-install-dir' -or [IO.File]::ReadAllText((Join-Path $target 'unknown.txt')) -cne 'keep') {
            throw "$mode unsafe Work diagnostic or preservation failed (exit $failureCode): $message"
        }
    }
    Remove-Item -LiteralPath (Join-Path $target 'unknown.txt')
    $ErrorActionPreference = 'Continue'
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --global-agent --skills-install-dir (Join-Path $scratch 'unsafe-global') 2>$null | Out-Null
    $unsafeGlobalExit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($unsafeGlobalExit -eq 0) { throw 'enabled Global AGENTS accepted linked directory' }
    $interactiveTarget = Join-Path (Join-Path $scratch 'custom Work') 'work'
    @('2', (Split-Path -Parent $interactiveTarget), 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --no-global-agent | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $interactiveTarget 'scripts/work.exe'))) {
        throw 'interactive custom Work directory install failed'
    }
    $globalWorkTarget = Join-Path (Join-Path $scratch 'global Work') 'work'
    $globalDirectory = Join-Path $scratch 'custom Global'
    @('2', $globalDirectory, 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --skills-install-dir (Split-Path -Parent $globalWorkTarget) --global-agent | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $globalWorkTarget 'scripts/work.exe')) -or
        -not (Test-Path -LiteralPath (Join-Path $globalDirectory 'AGENTS.md'))) {
        throw 'interactive custom Global AGENTS directory install failed'
    }
    & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --no-global-agent --skills-install-dir (Split-Path -Parent $target) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'same-version reinstall failed' }
    Write-TestReleaseInfo -Root $root -Version $olderVersion
    if (-not (Test-CoreRejected -Core $core -Root $root -Target $target)) { throw 'downgrade was accepted' }
    Write-TestReleaseInfo -Root $root -Version $baseVersion
    [IO.File]::WriteAllText((Join-Path $target 'unknown.txt'), 'keep')
    if (-not (Test-CoreRejected -Core $core -Root $root -Target $target) -or [IO.File]::ReadAllText((Join-Path $target 'unknown.txt')) -cne 'keep') { throw 'unknown content was changed' }
    Remove-Item -LiteralPath (Join-Path $target 'unknown.txt')
    $originalBinaryHash = (Get-FileHash -LiteralPath (Join-Path $target 'scripts/work.exe') -Algorithm SHA256).Hash
    $savedParentState = [IO.File]::ReadAllBytes($parentState)
    Remove-Item -LiteralPath $parentState
    if (-not (Test-CoreRejected -Core $core -Root $root -Target $target) -or (Get-FileHash -LiteralPath (Join-Path $target 'scripts/work.exe') -Algorithm SHA256).Hash -ne $originalBinaryHash) {
        throw 'installation without parent state changed Work'
    }
    @('YES', 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --no-global-agent --skills-install-dir (Split-Path -Parent $target) | Out-Null
    if ($LASTEXITCODE -eq 0 -or (Test-Path -LiteralPath $parentState)) {
        throw 'installation without parent state was taken over'
    }
    [IO.File]::WriteAllBytes($parentState, $savedParentState)
    $lockedBinary = [IO.File]::Open((Join-Path $target 'scripts/work.exe'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    try {
        if (-not (Test-CoreRejected -Core $core -Root $root -Target $target)) { throw 'in-use Work binary was accepted' }
    } finally { $lockedBinary.Dispose() }
    $crashInstaller = Join-Path $scratch 'crash-installer'
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/windows/installer') -Destination $crashInstaller -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/common/installer/options.tsv') -Destination (Join-Path $crashInstaller 'cli/options.tsv')
    $transactionSource = Join-Path $repoRoot 'os-scripts/windows/installer/transaction/work.ps1'
    $crashCore = Join-Path $crashInstaller 'install/install-core.ps1'
    $caseIndex = 0
    foreach ($point in @('staging', 'first-rename', 'second-rename', 'before-verified', 'after-verified')) {
        $caseIndex++
        $caseRoot = Join-Path $scratch "c$caseIndex"
        $crashTarget = Join-Path $caseRoot 'work'
        $crashGlobal = Join-Path $caseRoot 'global'
        Write-TestReleaseInfo -Root $root -Version $baseVersion
        & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --global-agent --global-agent-install-dir $crashGlobal --skills-install-dir (Split-Path -Parent $crashTarget) | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "initial $point Work installation failed" }
        $globalBefore = (Get-FileHash -LiteralPath (Join-Path $crashGlobal 'AGENTS.md') -Algorithm SHA256).Hash
        Write-TestReleaseInfo -Root $root -Version $nextVersion
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
        & powershell.exe -NoProfile -File $crashCore -Mode package -Root $root --non-interactive --global-agent --global-agent-install-dir $crashGlobal --skills-install-dir (Split-Path -Parent $crashTarget) 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) { throw "$point crash simulation completed successfully" }
        $transaction = @(Get-ChildItem -LiteralPath (Split-Path -Path $crashTarget -Parent) -Directory -Force |
            Where-Object { $_.Name -match '^\.work-install\.[0-9a-f]{32}$' })
        if ($transaction.Count -ne 1) { throw "$point crash did not leave one recovery record" }
        $oldPath = if (Test-Path -LiteralPath (Join-Path $transaction[0].FullName 'previous')) {
            Join-Path $transaction[0].FullName 'previous'
        } else { $crashTarget }
        $pendingState = Get-Content -LiteralPath (Join-Path (Split-Path -Parent $crashTarget) 'install-state.json') -Raw | ConvertFrom-Json
        if (-not (Test-Path -LiteralPath (Join-Path $oldPath 'scripts/work.exe')) -or
            $pendingState.status -ne 'installing' -or
            (Get-FileHash -LiteralPath (Join-Path $oldPath 'scripts/work.exe') -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath (Join-Path $root 'bin/work.exe') -Algorithm SHA256).Hash -or
            (Get-FileHash -LiteralPath (Join-Path $crashGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $globalBefore) {
            throw "$point crash lost previous Work or changed Global AGENTS"
        }
        & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --global-agent --global-agent-install-dir $crashGlobal --skills-install-dir (Split-Path -Parent $crashTarget) | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "$point crash recovery failed" }
        $crashState = Get-Content -LiteralPath (Join-Path (Split-Path -Parent $crashTarget) 'install-state.json') -Raw | ConvertFrom-Json
        if ($crashState.version -ne $nextVersion -or $crashState.status -ne 'installed' -or
            -not (Test-Path -LiteralPath (Join-Path $crashTarget 'scripts/work.exe')) -or
            (Test-Path -LiteralPath $transaction[0].FullName) -or
            (Get-FileHash -LiteralPath (Join-Path $crashGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $globalBefore) {
            throw "$point did not finish new Work installation or changed Global AGENTS"
        }
    }
    $originalGlobalSource = [IO.File]::ReadAllBytes((Join-Path $root 'AGENTS.md'))
    $caseIndex = 0
    foreach ($point in @('after-global', 'during-work', 'after-work-verified')) {
        $caseIndex++
        $caseRoot = Join-Path $scratch "g$caseIndex"
        $caseWork = Join-Path $caseRoot 'work'
        $caseGlobal = Join-Path $caseRoot 'global'
        [IO.File]::WriteAllBytes((Join-Path $root 'AGENTS.md'), $originalGlobalSource)
        Write-TestReleaseInfo -Root $root -Version $baseVersion
        & powershell.exe -NoProfile -File $core -Mode package -Root $root --non-interactive --global-agent --global-agent-install-dir $caseGlobal --skills-install-dir (Split-Path -Parent $caseWork) | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "$point initial combined installation failed" }
        $oldGlobalHash = (Get-FileHash -LiteralPath (Join-Path $caseGlobal 'AGENTS.md') -Algorithm SHA256).Hash
        [IO.File]::WriteAllText((Join-Path $root 'AGENTS.md'), "new Global AGENTS for $point`n")
        $newGlobalHash = (Get-FileHash -LiteralPath (Join-Path $root 'AGENTS.md') -Algorithm SHA256).Hash
        Write-TestReleaseInfo -Root $root -Version $nextVersion
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
        @('y', 'y') | & powershell.exe -NoProfile -File $crashCore -Mode package -Root $root --global-agent --global-agent-install-dir $caseGlobal --skills-install-dir (Split-Path -Parent $caseWork) 2>$null | Out-Null
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
        $restartOutput = @('y', 'y') | & powershell.exe -NoProfile -File $core -Mode package -Root $root --global-agent --global-agent-install-dir $caseGlobal --skills-install-dir (Split-Path -Parent $caseWork)
        if ($LASTEXITCODE -ne 0) { throw "$point combined recovery failed" }
        if ($point -ne 'after-work-verified' -and ($restartOutput -join "`n") -notmatch 'Global AGENTS.md differs') {
            throw "$point did not restore previous Global AGENTS before reinstall"
        }
        $finalState = Get-Content -LiteralPath (Join-Path $caseRoot 'install-state.json') -Raw | ConvertFrom-Json
        $remaining = @(Get-ChildItem -LiteralPath $caseRoot -Directory -Force |
            Where-Object { $_.Name -match '^\.(agents|work)-install\.' })
        if ($finalState.version -ne $nextVersion -or $finalState.status -ne 'installed' -or
            (Get-FileHash -LiteralPath (Join-Path $caseGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $newGlobalHash -or
            $remaining.Count -ne 0) { throw "$point left an incomplete combined transaction" }
    }
    $sourceProject = Join-Path $scratch 'source-project'
    New-Item -ItemType Directory -Path (Join-Path $sourceProject 'os-scripts') -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $sourceProject 'skills') -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $sourceProject 'rust') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'rust/Cargo.toml') -Destination (Join-Path $sourceProject 'rust/Cargo.toml')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/windows') -Destination (Join-Path $sourceProject 'os-scripts/windows') -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'skills/work') -Destination (Join-Path $sourceProject 'skills/work') -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'skills/tooling') -Destination (Join-Path $sourceProject 'skills/tooling') -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/common/installer/options.tsv') -Destination (Join-Path $sourceProject 'os-scripts/windows/installer/cli/options.tsv')
    New-Item -ItemType Directory -Path (Join-Path $sourceProject 'os-scripts/common/installer') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'os-scripts/common/installer/install-state.json') -Destination (Join-Path $sourceProject 'os-scripts/common/installer/install-state.json')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'AGENTS.md') -Destination (Join-Path $sourceProject 'AGENTS.md')
    [IO.File]::WriteAllText((Join-Path $sourceProject 'VERSION'), "$baseVersion`n")
    $cargoShim = Join-Path $scratch 'cargo-shim'
    New-Item -ItemType Directory -Path $cargoShim | Out-Null
    [IO.File]::WriteAllText((Join-Path $cargoShim 'cargo.cmd'),
        "@echo off`r`nif not exist `"%TEST_WORK_BINARY%`" (echo test Work binary is missing 1>&2 & exit /b 1)`r`nif not exist `"%TEST_TOOLING_BINARY%`" (echo test Tooling binary is missing 1>&2 & exit /b 1)`r`nif not exist target\release mkdir target\release`r`ncopy /Y `"%TEST_WORK_BINARY%`" target\release\work.exe >nul`r`ncopy /Y `"%TEST_TOOLING_BINARY%`" target\release\tooling.exe >nul`r`n")
    $env:TEST_WORK_BINARY = $binary
    $env:TEST_TOOLING_BINARY = Join-Path $repoRoot 'rust/target/release/tooling.exe'
    $env:PATH = "$cargoShim;$originalPath"
    $publicSourceEntry = Join-Path $sourceProject 'os-scripts/windows/install-work-from-source.bat'
    $publicWork = Join-Path $scratch 'public-source/work'
    $publicGlobal = Join-Path $scratch 'public-source/global'
    & $publicSourceEntry --non-interactive --global-agent --global-agent-install-dir $publicGlobal --skills-install-dir (Split-Path -Parent $publicWork) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'public Source initial installation failed' }
    $oldPublicGlobalHash = (Get-FileHash -LiteralPath (Join-Path $publicGlobal 'AGENTS.md') -Algorithm SHA256).Hash
    [IO.File]::WriteAllText((Join-Path $sourceProject 'VERSION'), "$nextVersion`n")
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
    @('y', 'y') | & $publicSourceEntry --global-agent --global-agent-install-dir $publicGlobal --skills-install-dir (Split-Path -Parent $publicWork) 2>$null | Out-Null
    $publicCrashExit = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    $publicTransactions = @(Get-ChildItem -LiteralPath (Split-Path -Path $publicWork -Parent) -Directory -Force |
        Where-Object { $_.Name -match '^\.agents-install\.[0-9a-f]{32}$' })
    $oldPublicState = Get-Content -LiteralPath (Join-Path (Split-Path -Parent $publicWork) 'install-state.json') -Raw | ConvertFrom-Json
    if ($publicCrashExit -eq 0 -or $publicTransactions.Count -ne 1 -or $oldPublicState.status -ne 'installing' -or
        (Get-FileHash -LiteralPath (Join-Path $publicTransactions[0].FullName 'global-previous') -Algorithm SHA256).Hash -ne $oldPublicGlobalHash -or
        (Get-FileHash -LiteralPath (Join-Path $publicGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $newPublicGlobalHash) {
        throw 'public Source interruption lost the previous install'
    }
    [IO.File]::WriteAllText($publicTransaction, $publicContents)
    @('y', 'y') | & $publicSourceEntry --global-agent --global-agent-install-dir $publicGlobal --skills-install-dir (Split-Path -Parent $publicWork) | Out-Null
    $publicState = Get-Content -LiteralPath (Join-Path (Split-Path -Parent $publicWork) 'install-state.json') -Raw | ConvertFrom-Json
    $remainingPublic = @(Get-ChildItem -LiteralPath (Split-Path -Path $publicWork -Parent) -Directory -Force |
        Where-Object { $_.Name -match '^\.(agents|work)-install\.' })
    if ($LASTEXITCODE -ne 0 -or $publicState.version -ne $nextVersion -or $publicState.status -ne 'installed' -or
        (Get-FileHash -LiteralPath (Join-Path $publicGlobal 'AGENTS.md') -Algorithm SHA256).Hash -ne $newPublicGlobalHash -or
        $remainingPublic.Count -ne 0) { throw 'public Source restart did not finish the transaction' }
    Write-Output 'Windows installer core fixture passed.'
} finally {
    $env:USERPROFILE = $originalUserProfile
    $env:HOME = $originalHome
    $env:PATH = $originalPath
    $env:TEST_WORK_BINARY = $originalTestWorkBinary
    $env:TEST_TOOLING_BINARY = $originalTestToolingBinary
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
