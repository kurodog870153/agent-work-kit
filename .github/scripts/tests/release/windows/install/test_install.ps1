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
        $output = & powershell.exe -NoProfile -File $core -Mode package -Root $root @Arguments --non-interactive --install-dir $cliDir 2>&1
        return [pscustomobject]@{ Code = $LASTEXITCODE; Output = ($output | Out-String) }
    } finally { foreach ($key in $Environment.Keys) { Remove-Item -Path "env:$key" -ErrorAction SilentlyContinue } }
}
function Remove-TestHostEntries {
    if ([string]::IsNullOrWhiteSpace($hostEntries)) { return }
    foreach ($name in 'work', 'tooling') {
        $link = Join-Path $hostEntries $name
        $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
        if ($null -ne $item) {
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { [IO.Directory]::Delete($link) }
            else { Remove-Item -LiteralPath $link -Recurse -Force }
        }
    }
}
function Get-Snapshot {
    $lines = foreach ($base in @($scratch + '\t', $scratch + '\g', $hostEntries)) {
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
    $hostEntries = Join-Path $env:USERPROFILE '.agents/skills'
    $globalDir = Join-Path $scratch 'g'
    $codex = @('--host', 'codex')

    $result = Run ($codex + @('--global-instructions', 'no', '--dry-run'))
    if ($result.Code -ne 0) { Fail "dry run: $($result.Output)" }
    if (Test-Path -LiteralPath (Join-Path $scratch 't')) { Fail 'dry run changed target' }
    $result = Run ($codex + @('--global-instructions', 'no')) @{ INSTALLER_TEST_FAILURE = 'apply-host-work' }
    if ($result.Code -eq 0 -or (Test-Path -LiteralPath $cliDir) -or (Test-Path -LiteralPath (Join-Path $hostEntries 'work'))) {
        Fail 'first Host entry failure was not rolled back'
    }
    $result = Run ($codex + @('--instructions-dir', $globalDir))
    if ($result.Code -ne 0) { Fail "first install: $($result.Output)" }
    foreach ($path in @('home\.agents\skills\work\SKILL.md', 'home\.agents\skills\tooling\SKILL.md', 't\agent-work-kit\skills\work\instructions\instructions.md', 't\agent-work-kit\skills\tooling\instructions\instructions.md', 't\agent-work-kit\skills\shared\bootstrap.md', 't\agent-work-kit\install-state.json',
            't\agent-work-kit\bin\work.exe', 't\agent-work-kit\bin\skill.exe', 'g\AGENTS.md')) {
        if (-not (Test-Path -LiteralPath (Join-Path $scratch $path) -PathType Leaf)) { Fail "missing $path" }
    }
    foreach ($name in 'work', 'tooling') {
        $entryDir = Join-Path $hostEntries $name
        $entry = Get-Item -LiteralPath (Join-Path $entryDir 'SKILL.md') -Force
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            ((Get-Item -LiteralPath $entryDir -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            @(Get-ChildItem -LiteralPath $entryDir -Force).Count -ne 1) { Fail 'invalid physical Host entry' }
    }
    foreach ($path in @('t\agent-work-kit\skills\work\SKILL.md', 't\agent-work-kit\skills\tooling\SKILL.md', 't\agent-work-kit\skills\contract', 't\agent-work-kit\skills\shared\SKILL.md', 't\agent-work-kit\skills\work\scripts', 't\agent-work-kit\skills\.agent-work-kit-txn')) {
        if (Test-Path -LiteralPath (Join-Path $scratch $path)) { Fail "unexpected $path" }
    }
    $result = Run ($codex + @('--instructions-dir', $globalDir))
    if ($result.Code -ne 0 -or $result.Output -notmatch 'unchanged') { Fail "reinstall: $($result.Output)" }

    # Reject legacy links and missing or modified physical entries, preserving foreign content.
    $workEntry = Join-Path $hostEntries 'work'
    $entryFile = Join-Path $workEntry 'SKILL.md'
    $entryBytes = [IO.File]::ReadAllBytes($entryFile)
    $foreign = Join-Path $scratch 'foreign'
    New-Item -ItemType Directory -Path $foreign | Out-Null
    [IO.File]::WriteAllText((Join-Path $foreign 'keep.txt'), 'keep')
    Remove-Item -LiteralPath $workEntry -Recurse -Force
    New-Item -ItemType Junction -Path $workEntry -Target $foreign | Out-Null
    if ((Run ($codex + @('--instructions-dir', $globalDir))).Code -eq 0) { Fail 'accepted legacy Junction' }
    if ((Get-Item -LiteralPath $workEntry -Force).LinkType -cne 'Junction' -or [IO.File]::ReadAllText((Join-Path $foreign 'keep.txt')) -cne 'keep') { Fail 'legacy entry or target changed' }
    [IO.Directory]::Delete($workEntry)
    if ((Run ($codex + @('--instructions-dir', $globalDir))).Code -eq 0) { Fail 'repaired missing discovery entry' }
    New-Item -ItemType Directory -Path $workEntry | Out-Null
    [IO.File]::WriteAllBytes($entryFile, $entryBytes)
    [IO.File]::AppendAllText($entryFile, 'tampered')
    $before = Get-Snapshot
    if ((Run ($codex + @('--instructions-dir', $globalDir))).Code -eq 0 -or (Get-Snapshot) -ne $before) { Fail 'accepted or changed modified entry' }
    [IO.File]::WriteAllBytes($entryFile, $entryBytes)
    $before = Get-Snapshot
    foreach ($stage in 'prepare', 'apply-work-moved', 'apply-work', 'apply-shared', 'apply-bin', 'apply-host-work-moved', 'apply-host-work', 'apply-host-tooling', 'verify') {
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
    foreach ($stage in 'apply-work', 'apply-bin', 'apply-host-work', 'apply-host-tooling', 'verify', 'commit', 'after-commit') {
        $result = Run ($codex + @('--instructions-dir', $globalDir)) @{ INSTALLER_TEST_CRASH = $stage }
        if ($result.Code -ne 97) { Fail "crash injection $stage ($($result.Code))" }
        $result = Run ($codex + @('--instructions-dir', $globalDir))
        if ($result.Code -ne 0) { Fail "recovery after ${stage}: $($result.Output)" }
        if (Test-Path -LiteralPath (Join-Path $cliDir '.agent-work-kit-txn')) { Fail "transaction left after $stage" }
    }

    # Unknown content is never taken over.
    Remove-TestHostEntries
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
    Add-Content -LiteralPath (Join-Path $hostEntries 'work\SKILL.md') -Value 'x'
    $before = Get-Snapshot
    if ((Run ($codex + @('--global-instructions', 'no'))).Code -eq 0) { Fail 'accepted modified installation' }
    if ((Get-Snapshot) -ne $before) { Fail 'modified installation changed' }
    Remove-TestHostEntries
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
            $crossEntries = Join-Path $crossHome '.agents/skills'
            $result = Run ($codex + @('--global-instructions', 'no'))
            if ($result.Code -ne 0) { Fail "cross-drive first install: $($result.Output)" }
            foreach ($name in 'work', 'tooling') {
                $entry = Join-Path $crossEntries "$name/SKILL.md"
                if (-not (Test-Path -LiteralPath $entry -PathType Leaf) -or
                    ((Get-Item -LiteralPath $entry -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                    Fail "cross-drive physical Host entry missing: $entry"
                }
            }
            $result = Run ($codex + @('--global-instructions', 'no'))
            if ($result.Code -ne 0) { Fail "cross-drive reinstall: $($result.Output)" }
            $result = Run ($codex + @('--global-instructions', 'no')) @{ INSTALLER_TEST_CRASH = 'apply-host-work' }
            if ($result.Code -ne 97) { Fail 'cross-drive Host entry interruption was not injected' }
            $result = Run ($codex + @('--global-instructions', 'no'))
            if ($result.Code -ne 0) { Fail "cross-drive Host entry recovery: $($result.Output)" }
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
            Write-Output 'Windows cross-drive physical entries and Global Instructions crash recovery passed.'
        } finally {
            $env:USERPROFILE = $previousProfile
            if (Test-Path -LiteralPath $cliDir) { Remove-Item -LiteralPath $cliDir -Recurse -Force }
            if (Test-Path -LiteralPath $crossHome) { Remove-Item -LiteralPath $crossHome -Recurse -Force }
        }
    } else {
        Write-Output 'Windows cross-drive test skipped: no separate Home volume.'
    }
    Write-Output 'Windows installer cases passed.'
} finally {
    Remove-TestHostEntries
    $env:USERPROFILE = $originalProfile
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
