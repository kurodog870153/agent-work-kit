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
        $output = & pwsh -NoProfile -File $core -Mode package -Root $root @Arguments --non-interactive --skills-dir $skills 2>&1
        return [pscustomobject]@{ Code = $LASTEXITCODE; Output = ($output | Out-String) }
    } finally { foreach ($key in $Environment.Keys) { Remove-Item -Path "env:$key" -ErrorAction SilentlyContinue } }
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
    $skills = Join-Path $scratch 't\skills'
    $cliDir = Join-Path $scratch 't\agent-work-kit'
    $globalDir = Join-Path $scratch 'g'
    $codex = @('--host', 'codex')

    $result = Run ($codex + @('--global-instructions', 'no', '--dry-run'))
    if ($result.Code -ne 0) { Fail "dry run: $($result.Output)" }
    if (Test-Path -LiteralPath (Join-Path $scratch 't')) { Fail 'dry run changed target' }
    $result = Run ($codex + @('--instructions-dir', $globalDir))
    if ($result.Code -ne 0) { Fail "first install: $($result.Output)" }
    foreach ($path in @('t\skills\work\SKILL.md', 't\skills\tooling\SKILL.md', 't\skills\shared\bootstrap.md', 't\skills\install-state.json',
            't\agent-work-kit\bin\work.exe', 't\agent-work-kit\bin\skill.exe', 'g\AGENTS.md')) {
        if (-not (Test-Path -LiteralPath (Join-Path $scratch $path) -PathType Leaf)) { Fail "missing $path" }
    }
    foreach ($path in @('t\skills\contract', 't\skills\shared\SKILL.md', 't\skills\work\scripts', 't\skills\.agent-work-kit-txn')) {
        if (Test-Path -LiteralPath (Join-Path $scratch $path)) { Fail "unexpected $path" }
    }
    $result = Run ($codex + @('--instructions-dir', $globalDir))
    if ($result.Code -ne 0 -or $result.Output -notmatch 'unchanged') { Fail "reinstall: $($result.Output)" }

    $before = Get-Snapshot
    foreach ($stage in 'prepare', 'apply-work-moved', 'apply-work', 'apply-shared', 'apply-bin', 'verify') {
        $result = Run ($codex + @('--instructions-dir', $globalDir)) @{ INSTALLER_TEST_FAILURE = $stage }
        if ($result.Code -eq 0) { Fail "failure at $stage was ignored" }
        if ((Get-Snapshot) -ne $before -or (Test-Path -LiteralPath (Join-Path $skills '.agent-work-kit-txn'))) { Fail "failure at $stage did not restore" }
    }
    # apply-global is only reached when Global Instructions change: our own previous copy is replaceable.
    $globalSource = Join-Path $root 'instructions/global-instructions.md'
    $originalGlobal = [IO.File]::ReadAllBytes($globalSource)
    Add-Content -LiteralPath $globalSource -Value 'updated'
    $beforeGlobal = Get-Snapshot
    $result = Run ($codex + @('--instructions-dir', $globalDir)) @{ INSTALLER_TEST_FAILURE = 'apply-global' }
    if ($result.Code -eq 0) { Fail 'failure at apply-global was ignored' }
    if ((Get-Snapshot) -ne $beforeGlobal -or (Test-Path -LiteralPath (Join-Path $skills '.agent-work-kit-txn'))) { Fail 'failure at apply-global did not restore' }
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
        if (Test-Path -LiteralPath (Join-Path $skills '.agent-work-kit-txn')) { Fail "transaction left after $stage" }
    }

    # Unknown content is never taken over.
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
    Remove-Item -LiteralPath (Join-Path $scratch 't') -Recurse -Force
    if ((Run @('--host', 'claude')).Code -eq 0) { Fail 'Host mismatch accepted' }
    if (Test-Path -LiteralPath (Join-Path $scratch 't')) { Fail 'Host mismatch changed target' }
    Write-Output 'Windows installer cases passed.'
} finally {
    $env:USERPROFILE = $originalProfile
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
