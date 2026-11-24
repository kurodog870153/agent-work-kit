$ErrorActionPreference = 'Stop'
# Installer core scenarios on a temporary target with stub binaries (fixtures are generated here).
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
$core = Join-Path $repoRoot 'os-scripts/windows/installer/install/install-core.ps1'
$scratch = Join-Path $repoRoot ('.installer-test-core.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolved = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolved.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolved) -match '^\.installer-test-core\.[0-9a-f]{32}$')) { throw 'unsafe cleanup path' }
$originalProfile = $env:USERPROFILE

function Fail([string] $Message) { throw "FAIL: $Message" }
function Run([string[]] $Arguments, [hashtable] $Environment = @{}) {
    foreach ($key in $Environment.Keys) { Set-Item -Path "env:$key" -Value $Environment[$key] }
    # Expected failures write to stderr; with 'Stop' the merged stderr would throw instead of returning the exit code.
    $ErrorActionPreference = 'Continue'
    try {
        $output = & pwsh -NoProfile -File $core -Mode package -Root $root @Arguments --non-interactive --install-dir $cliDir 2>&1
        return [pscustomobject]@{ Code = $LASTEXITCODE; Output = ($output | Out-String) }
    } finally { foreach ($key in $Environment.Keys) { Remove-Item -Path "env:$key" -ErrorAction SilentlyContinue } }
}
function Remove-TestHostLinks {
    if ([string]::IsNullOrWhiteSpace($hostLinks)) { return }
    foreach ($name in 'work', 'tooling') {
        $link = Join-Path $hostLinks $name
        $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
        if ($null -ne $item) {
            if ($item.LinkType -cne 'Junction') { Fail "unexpected Host discovery entry: $link" }
            [IO.Directory]::Delete($link)
        }
    }
}
function Get-Snapshot {
    $lines = foreach ($base in @($scratch + '\t', $scratch + '\g')) {
        if (Test-Path -LiteralPath $base) {
            Get-ChildItem -LiteralPath $base -Recurse -File -Force | ForEach-Object {
                "$($_.FullName.Substring($scratch.Length))  $((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash)"
            }
        }
    }
    return (@($lines) | Sort-Object) -join "`n"
}

try {
    $root = Join-Path $scratch 'package'
    New-Item -ItemType Directory -Path (Join-Path $root 'bin'), (Join-Path $root 'instructions'), (Join-Path $scratch 'home') | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'skills') -Destination (Join-Path $root 'skills') -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'instructions/global-instructions.md') -Destination (Join-Path $root 'instructions')
    foreach ($name in 'work', 'skill') {
        Copy-Item -LiteralPath (Join-Path $repoRoot "rust/target/release/$name.exe") -Destination (Join-Path $root "bin/$name.exe")
    }
    $info = '{"host":"codex","version":"2.5.0","rust_workspace_version":"3.0.0","target":"x86_64-pc-windows-msvc"}'
    [IO.File]::WriteAllText((Join-Path $root 'release-info.json'), $info)
    $env:USERPROFILE = Join-Path $scratch 'home'
    $cliDir = Join-Path $scratch 't\agent-work-kit'
    $skills = Join-Path $cliDir 'skills'
    $hostLinks = Join-Path $env:USERPROFILE '.agents/skills'
    $globalDir = Join-Path $scratch 'g'
    $codex = @('--host', 'codex')

    $result = Run ($codex + @('--global-instructions', 'no', '--dry-run'))
    if ($result.Code -ne 0) { Fail "dry run: $($result.Output)" }
    if (Test-Path -LiteralPath (Join-Path $scratch 't')) { Fail 'dry run changed target' }
    $result = Run ($codex + @('--global-instructions', 'no')) @{ INSTALLER_TEST_FAILURE = 'link-create-denied' }
    if ($result.Code -eq 0 -or (Test-Path -LiteralPath $cliDir) -or (Test-Path -LiteralPath (Join-Path $hostLinks 'work'))) {
        Fail 'simulated Junction creation failure was not rolled back'
    }
    $result = Run ($codex + @('--global-instructions', 'no')) @{ INSTALLER_TEST_FAILURE = 'apply-link-work' }
    if ($result.Code -eq 0 -or (Test-Path -LiteralPath $cliDir)) { Fail 'first link failure was not rolled back' }
    $result = Run ($codex + @('--instructions-dir', $globalDir))
    if ($result.Code -ne 0) { Fail "first install: $($result.Output)" }
    foreach ($path in @('t\agent-work-kit\skills\work\SKILL.md', 't\agent-work-kit\skills\tooling\SKILL.md', 't\agent-work-kit\skills\shared\bootstrap.md', 't\agent-work-kit\install-state.json',
            't\agent-work-kit\bin\work.exe', 't\agent-work-kit\bin\skill.exe', 'g\AGENTS.md')) {
        if (-not (Test-Path -LiteralPath (Join-Path $scratch $path) -PathType Leaf)) { Fail "missing $path" }
    }
    if (-not (Test-Path -LiteralPath (Join-Path $hostLinks 'work') -PathType Container)) { Fail 'Work discovery link missing' }
    foreach ($path in @('t\agent-work-kit\skills\contract', 't\agent-work-kit\skills\shared\SKILL.md', 't\agent-work-kit\skills\work\scripts', 't\agent-work-kit\skills\.agent-work-kit-txn')) {
        if (Test-Path -LiteralPath (Join-Path $scratch $path)) { Fail "unexpected $path" }
    }
    $result = Run ($codex + @('--instructions-dir', $globalDir))
    if ($result.Code -ne 0 -or $result.Output -notmatch 'unchanged') { Fail "reinstall: $($result.Output)" }

    # A valid Junction is reused; a repointed Junction or legacy SymbolicLink is never adopted.
    $workLink = Join-Path $hostLinks 'work'
    $expectedWork = Join-Path $skills 'work'
    $foreign = Join-Path $scratch 'foreign'
    New-Item -ItemType Directory -Path $foreign | Out-Null
    [IO.File]::WriteAllText((Join-Path $foreign 'keep.txt'), 'keep')
    [IO.Directory]::Delete($workLink)
    New-Item -ItemType Junction -Path $workLink -Target $foreign | Out-Null
    $before = Get-Snapshot
    if ((Run ($codex + @('--instructions-dir', $globalDir))).Code -eq 0) { Fail 'accepted wrong Junction' }
    if ((Get-Snapshot) -ne $before -or [IO.File]::ReadAllText((Join-Path $foreign 'keep.txt')) -cne 'keep') { Fail 'wrong Junction target changed' }
    [IO.Directory]::Delete($workLink)
    if ((Run ($codex + @('--instructions-dir', $globalDir))).Code -eq 0) { Fail 'repaired missing discovery link' }
    New-Item -ItemType SymbolicLink -Path $workLink -Target $expectedWork | Out-Null
    try {
        if ((Run ($codex + @('--instructions-dir', $globalDir))).Code -eq 0) { Fail 'accepted legacy SymbolicLink' }
        if ((Get-Item -LiteralPath $workLink -Force).LinkType -cne 'SymbolicLink') { Fail 'legacy link was changed' }
    } finally { [IO.Directory]::Delete($workLink) }
    New-Item -ItemType Junction -Path $workLink -Target $expectedWork | Out-Null
    $before = Get-Snapshot
    foreach ($stage in 'prepare', 'apply-work-moved', 'apply-work', 'apply-shared', 'apply-bin', 'verify') {
        $result = Run ($codex + @('--instructions-dir', $globalDir)) @{ INSTALLER_TEST_FAILURE = $stage }
        if ($result.Code -eq 0) { Fail "failure at $stage was ignored" }
        if ((Get-Snapshot) -ne $before -or (Test-Path -LiteralPath (Join-Path $cliDir '.agent-work-kit-txn'))) { Fail "failure at $stage did not restore" }
    }
    # apply-global is only reached when Global Instructions change: our own previous copy is replaceable.
    $globalSource = Join-Path $root 'instructions/global-instructions.md'
    $originalGlobal = [IO.File]::ReadAllBytes($globalSource)
    Add-Content -LiteralPath $globalSource -Value 'updated'
    $beforeGlobal = Get-Snapshot
    $result = Run ($codex + @('--instructions-dir', $globalDir)) @{ INSTALLER_TEST_FAILURE = 'apply-global' }
    if ($result.Code -eq 0) { Fail 'failure at apply-global was ignored' }
    if ((Get-Snapshot) -ne $beforeGlobal -or (Test-Path -LiteralPath (Join-Path $cliDir '.agent-work-kit-txn'))) { Fail 'failure at apply-global did not restore' }
    $result = Run ($codex + @('--instructions-dir', $globalDir))
    if ($result.Code -ne 0) { Fail "replace own Global Instructions: $($result.Output)" }
    if ((Get-FileHash -LiteralPath $globalSource).Hash -ne (Get-FileHash -LiteralPath (Join-Path $globalDir 'AGENTS.md')).Hash) { Fail 'Global Instructions not replaced' }
    [IO.File]::WriteAllBytes($globalSource, $originalGlobal)
    $result = Run ($codex + @('--instructions-dir', $globalDir))
    if ($result.Code -ne 0) { Fail "restore Global Instructions: $($result.Output)" }
    foreach ($stage in 'apply-work', 'apply-bin', 'verify', 'commit', 'after-commit') {
        $result = Run ($codex + @('--instructions-dir', $globalDir)) @{ INSTALLER_TEST_CRASH = $stage }
        if ($result.Code -ne 97) { Fail "crash injection $stage ($($result.Code))" }
        $result = Run ($codex + @('--instructions-dir', $globalDir))
        if ($result.Code -ne 0) { Fail "recovery after ${stage}: $($result.Output)" }
        if (Test-Path -LiteralPath (Join-Path $cliDir '.agent-work-kit-txn')) { Fail "transaction left after $stage" }
    }

    # Unknown content is never taken over.
    Remove-TestHostLinks
    Remove-Item -LiteralPath (Join-Path $scratch 't'), $globalDir -Recurse -Force
    New-Item -ItemType Directory -Path (Join-Path $skills 'work') | Out-Null
    [IO.File]::WriteAllText((Join-Path $skills 'work\notes.txt'), 'mine')
    $before = Get-Snapshot
    if ((Run ($codex + @('--global-instructions', 'no'))).Code -eq 0) { Fail 'accepted unknown Work content' }
    if ((Get-Snapshot) -ne $before) { Fail 'unknown content changed' }
    Remove-Item -LiteralPath (Join-Path $scratch 't') -Recurse -Force
    New-Item -ItemType Directory -Path $globalDir | Out-Null
    [IO.File]::WriteAllText((Join-Path $globalDir 'AGENTS.md'), "user`n")
    if ((Run ($codex + @('--instructions-dir', $globalDir))).Code -eq 0) { Fail 'replaced differing Global Instructions' }
    if ([IO.File]::ReadAllText((Join-Path $globalDir 'AGENTS.md')) -ne "user`n" -or (Test-Path -LiteralPath (Join-Path $skills 'work'))) { Fail 'Global Instructions run changed state' }
    if ((Run ($codex + @('--global-instructions', 'no'))).Code -ne 0) { Fail 'install without Global Instructions' }
    Add-Content -LiteralPath (Join-Path $skills 'work\SKILL.md') -Value 'x'
    $before = Get-Snapshot
    if ((Run ($codex + @('--global-instructions', 'no'))).Code -eq 0) { Fail 'accepted modified installation' }
    if ((Get-Snapshot) -ne $before) { Fail 'modified installation changed' }
    Remove-TestHostLinks
    Remove-Item -LiteralPath (Join-Path $scratch 't') -Recurse -Force
    if ((Run @('--host', 'claude')).Code -eq 0) { Fail 'Host mismatch accepted' }
    if (Test-Path -LiteralPath (Join-Path $scratch 't')) { Fail 'Host mismatch changed target' }

    # Cross-drive discovery must work with a custom install root without relying
    # on moving Skills or CLI binaries between volumes.
    if (-not [string]::IsNullOrWhiteSpace($originalProfile) -and
        [IO.Path]::GetPathRoot($originalProfile) -ine [IO.Path]::GetPathRoot($cliDir)) {
        $crossHome = Join-Path $originalProfile ('.issue-134-cross-drive-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $crossHome -ErrorAction Stop | Out-Null
        $previousProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = $crossHome
            $crossLinks = Join-Path $crossHome '.agents/skills'
            $result = Run ($codex + @('--global-instructions', 'no'))
            if ($result.Code -ne 0) { Fail "cross-drive first install: $($result.Output)" }
            foreach ($name in 'work', 'tooling') {
                $link = Join-Path $crossLinks $name
                $item = Get-Item -LiteralPath $link -Force -ErrorAction Stop
                if ($item.LinkType -cne 'Junction' -or -not (Test-Path -LiteralPath $link -PathType Container)) {
                    Fail "cross-drive Host discovery link missing: $link"
                }
                if (-not [IO.Path]::IsPathRooted([string] @($item.Target)[0])) { Fail 'cross-drive link target is not absolute' }
            }
            $result = Run ($codex + @('--global-instructions', 'no'))
            if ($result.Code -ne 0) { Fail "cross-drive reinstall: $($result.Output)" }
            # Global Instructions live on the Host volume, while the transaction
            # (including its backup) is on the installation volume.
            $crossGlobalDir = Join-Path $crossHome '.codex'
            $crossOptions = $codex + @('--global-instructions', 'yes', '--instructions-dir', $crossGlobalDir)
            $result = Run $crossOptions
            if ($result.Code -ne 0) { Fail "cross-drive Global Instructions install: $($result.Output)" }
            $crossGlobalTarget = Join-Path $crossGlobalDir 'AGENTS.md'
            if (-not (Test-Path -LiteralPath $crossGlobalTarget -PathType Leaf)) { Fail 'cross-drive Global Instructions missing' }
            $crossGlobalHash = (Get-FileHash -LiteralPath $crossGlobalTarget -Algorithm SHA256).Hash
            [IO.File]::AppendAllText($globalSource, "`n# cross-drive test update`n")
            $result = Run $crossOptions @{ INSTALLER_TEST_CRASH = 'apply-global-moved' }
            if ($result.Code -ne 97) { Fail "cross-drive crash injection did not interrupt Global replacement: $($result.Output)" }
            $result = Run $crossOptions
            if ($result.Code -ne 0) { Fail "cross-drive Global replacement recovery: $($result.Output)" }
            if ((Get-FileHash -LiteralPath $crossGlobalTarget -Algorithm SHA256).Hash -eq $crossGlobalHash) {
                Fail 'cross-drive Global replacement did not publish the new source'
            }
            if (Test-Path -LiteralPath (Join-Path $cliDir '.agent-work-kit-txn')) { Fail 'cross-drive Recovery left a transaction' }
            [IO.File]::WriteAllBytes($globalSource, $originalGlobal)
            Write-Output 'Windows cross-drive Junction and Global Instructions crash recovery passed.'
        } finally {
            foreach ($name in 'work', 'tooling') {
                $link = Join-Path $crossHome ".agents/skills/$name"
                $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
                if ($null -ne $item -and $item.LinkType -ceq 'Junction') { [IO.Directory]::Delete($link) }
            }
            $env:USERPROFILE = $previousProfile
            if (Test-Path -LiteralPath $cliDir) { Remove-Item -LiteralPath $cliDir -Recurse -Force }
            if (Test-Path -LiteralPath $crossHome) { Remove-Item -LiteralPath $crossHome -Recurse -Force }
        }
    } else {
        Write-Output 'Windows cross-drive test skipped: no separate Home volume.'
    }
    Write-Output 'Windows installer cases passed.'
} finally {
    Remove-TestHostLinks
    $env:USERPROFILE = $originalProfile
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
