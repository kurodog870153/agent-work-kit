Set-StrictMode -Version Latest

function Get-InstallerReleaseField {
    param([string] $Path, [string] $Name)
    try { $info = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($Path)) -ErrorAction Stop } catch { throw 'release-info.json is missing or invalid' }
    $value = $info.PSObject.Properties[$Name]
    if ($null -eq $value -or $value.Value -isnot [string] -or [string]::IsNullOrEmpty($value.Value)) { throw 'release-info.json is missing or invalid' }
    return $value.Value
}

# Copies the verified sources into the transaction. Shared is copied once and used by both Skills.
function Copy-InstallerPayload {
    param([string] $Root, [string] $WorkBinary, [string] $SkillBinary, [string] $GlobalSource, [string] $GlobalAction)
    $staged = Join-Path $script:InstallerTx 'staged'
    foreach ($name in 'work', 'tooling', 'shared') {
        Copy-Item -LiteralPath (Join-Path $Root "skills/$name") -Destination (Join-Path $staged $name) -Recurse -ErrorAction Stop
    }
    New-Item -ItemType Directory -Path (Join-Path $staged 'bin') -ErrorAction Stop | Out-Null
    Copy-Item -LiteralPath $WorkBinary -Destination (Join-Path $staged 'bin/work.exe') -ErrorAction Stop
    Copy-Item -LiteralPath $SkillBinary -Destination (Join-Path $staged 'bin/skill.exe') -ErrorAction Stop
    if ($GlobalAction -in @('install', 'replace')) { Copy-Item -LiteralPath $GlobalSource -Destination (Join-Path $staged 'global') -ErrorAction Stop }
}

# Expected entries of the new installation: managed files plus the Global Instructions copy it owns.
function Get-InstallerNewManifest {
    param([string] $GlobalDir, [string] $GlobalTarget, [string] $GlobalSourceSha)
    $staged = Join-Path $script:InstallerTx 'staged'
    $entries = @(Get-InstallerActualManifest -InstallDir $staged -Staged)
    if ($entries.Count -eq 0) { throw 'no installation resources were prepared' }
    if (-not [string]::IsNullOrEmpty($GlobalDir)) { $entries += "$GlobalSourceSha  global/$([IO.Path]::GetFileName($GlobalTarget))" }
    return $entries
}

# Checks the installed result: exact managed content, runnable binaries and the Global Instructions copy.
function Test-InstallerInstallation {
    param([string[]] $Entries, [string] $GlobalDir, [string] $GlobalTarget, [string] $GlobalSourceSha)
    if (-not (Test-InstallerManifestMatches -InstallDir $script:InstallerInstallDir -Entries $Entries)) { return $false }
    foreach ($name in 'work', 'tooling') {
        if (-not (Test-InstallerHostLink -Name $name -InstallDir $script:InstallerInstallDir -HostSkillsDir $script:InstallerHostSkillsDir)) { return $false }
    }
    foreach ($name in 'work', 'skill') {
        & (Join-Path $script:InstallerCliDir "bin/$name.exe") --help *> $null
        if ($LASTEXITCODE -ne 0) { return $false }
    }
    if (-not [string]::IsNullOrEmpty($GlobalDir)) {
        if (-not (Test-Path -LiteralPath $GlobalTarget -PathType Leaf) -or (Test-InstallerReparse (Get-Item -LiteralPath $GlobalTarget -Force)) -or
            (Get-InstallerFileSha256 -Path $GlobalTarget) -cne $GlobalSourceSha) { return $false }
    }
    return $true
}
