Set-StrictMode -Version Latest

function Get-InstallerMigrationRecordPath {
    param([string] $Target)
    return Join-Path (Split-Path -Path $Target -Parent) '.work-migration-cleanup'
}

function Get-InstallerMigrationTreeHash {
    param([string] $Path)
    $entries = [Collections.Generic.List[string]]::new()
    foreach ($entry in @(Get-ChildItem -LiteralPath $Path -Recurse -File -Force | Sort-Object FullName)) {
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'unsafe previous Work file' }
        $relative = $entry.FullName.Substring($Path.TrimEnd('\', '/').Length + 1).Replace('\', '/')
        $entries.Add("$(Get-InstallerWorkFileSha256 $entry.FullName)  $relative")
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($entries -join "`n") + "`n")
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant() }
    finally { $algorithm.Dispose() }
}

function Write-InstallerMigrationRecord {
    param([string] $Target, [string] $Old, [string] $SourceWork, [string] $StateJson)
    $inspection = Inspect-InstallerTarget -Target $Old -SourceWork $SourceWork -StateValid $true
    if ($inspection.Kind -ne 'managed') { throw 'previous Work cannot be identified for migration' }
    $record = Get-InstallerMigrationRecordPath -Target $Target
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $stateHash = ([BitConverter]::ToString($algorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($StateJson)))).Replace('-', '').ToLowerInvariant()
    } finally { $algorithm.Dispose() }
    $data = [pscustomobject]@{ target = $Target; old = $Old; state_sha256 = $stateHash;
        tree_sha256 = (Get-InstallerMigrationTreeHash -Path $Old) }
    if (Test-Path -LiteralPath $record) {
        $item = Get-Item -LiteralPath $record -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'unsafe migration cleanup record'
        }
        $existing = [IO.File]::ReadAllText($record) | ConvertFrom-Json
        if ($existing.target -cne $Target -or $existing.old -cne $Old -or
            $existing.tree_sha256 -cne $data.tree_sha256) {
            throw 'existing migration cleanup record does not match previous Work'
        }
    }
    $temporary = "$record.$([guid]::NewGuid().ToString('N'))"
    try {
        [IO.File]::WriteAllText($temporary, ($data | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
        if (Test-Path -LiteralPath $record) { [IO.File]::Replace($temporary, $record, $null) }
        else { Move-Item -LiteralPath $temporary -Destination $record -ErrorAction Stop }
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
}

function Invoke-InstallerMigrationCleanup {
    param([string] $Target, [string] $SourceWork)
    $record = Get-InstallerMigrationRecordPath -Target $Target
    if (-not (Test-Path -LiteralPath $record)) { return }
    try {
        $item = Get-Item -LiteralPath $record -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'unsafe cleanup record' }
        $data = [IO.File]::ReadAllText($record) | ConvertFrom-Json
        if ($data.target -cne $Target -or -not [IO.Path]::IsPathRooted($data.old) -or
            [string]::Equals($data.old, $Target, [StringComparison]::OrdinalIgnoreCase) -or
            $data.state_sha256 -cnotmatch '^[0-9a-f]{64}$' -or $data.tree_sha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw 'invalid cleanup record'
        }
        if (-not (Test-Path -LiteralPath $data.old)) {
            Remove-Item -LiteralPath $record -Force -ErrorAction Stop
            return
        }
        $newInspection = Inspect-InstallerTarget -Target $Target -SourceWork $SourceWork -StateValid $true
        if ($newInspection.Kind -ne 'managed' -or
            (Get-InstallerWorkFileSha256 (Join-Path $Target 'install-state.json')) -cne $data.state_sha256) {
            throw 'migration target changed or is not verified'
        }
        $oldInspection = Inspect-InstallerTarget -Target $data.old -SourceWork $SourceWork -StateValid $true
        if ($oldInspection.Kind -ne 'managed' -or
            (Get-InstallerMigrationTreeHash -Path $data.old) -cne $data.tree_sha256) {
            throw 'previous Work contents changed'
        }
        if (Test-InstallerBinaryInUse -Binary (Join-Path $data.old 'scripts/work.exe')) { throw 'previous Work is in use' }
        foreach ($file in @(Get-ChildItem -LiteralPath $data.old -Recurse -File -Force)) {
            $stream = [IO.File]::Open($file.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
            $stream.Dispose()
        }
        Remove-Item -LiteralPath $data.old -Recurse -Force -ErrorAction Stop
        Remove-Item -LiteralPath $record -Force -ErrorAction Stop
    } catch { Write-Warning "Previous Work awaits verified cleanup: $record. $_" }
}
