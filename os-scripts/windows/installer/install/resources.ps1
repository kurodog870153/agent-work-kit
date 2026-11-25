Set-StrictMode -Version Latest

function Get-InstallerReleaseField {
    param([string] $Path, [string] $Name)
    try { $info = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($Path)) -ErrorAction Stop } catch { throw 'release-info.json is missing or invalid' }
    $value = $info.PSObject.Properties[$Name]
    if ($null -eq $value -or $value.Value -isnot [string] -or [string]::IsNullOrEmpty($value.Value)) { throw 'release-info.json is missing or invalid' }
    return $value.Value
}

# Generate the physical entry from the existing source template.
function Write-InstallerSkillEntry {
    param([string] $Root, [string] $Name, [string] $Destination)
    $paths = @{
        SHARED_BOOTSTRAP = (Join-Path $script:InstallerSkillsDir 'shared/bootstrap.md')
        SKILL_ROOT = (Join-Path $script:InstallerSkillsDir "$Name/instructions/instructions.md")
        SKILL_CLI = (Join-Path $script:InstallerCliDir 'bin/skill.exe')
        SKILL_INSTALL = $script:InstallerSkillsDir
        INSTALL_STATE = (Join-Path $script:InstallerInstallDir 'install-state.json')
    }
    if ($Name -ceq 'work') { $paths.WORK_CLI = Join-Path $script:InstallerCliDir 'bin/work.exe' }
    $template = [IO.File]::ReadAllText((Join-Path $Root "skills/$Name/SKILL.md"))
    $matches = [regex]::Matches($template, '\{\{([A-Z_]+)\}\}')
    if ($matches.Count -ne $paths.Count) { throw "invalid Skill entry template: $Name" }
    foreach ($key in $paths.Keys) {
        if (@($matches | Where-Object { $_.Groups[1].Value -ceq $key }).Count -ne 1) { throw "missing or duplicate Skill path: $key" }
        if (-not (Test-InstallerAbsolutePath $paths[$key])) { throw "Skill path is not absolute: $key" }
    }
    $entry = [regex]::Replace($template, '\{\{([A-Z_]+)\}\}', [Text.RegularExpressions.MatchEvaluator] {
        param($match)
        ConvertTo-Json -InputObject ([string] $paths[$match.Groups[1].Value]) -Compress
    })
    [IO.File]::WriteAllText($Destination, $entry, [Text.UTF8Encoding]::new($false))
}

# Copies the verified sources into the transaction. Shared is copied once and used by both Skills.
function Copy-InstallerPayload {
    param([string] $Root, [string] $WorkBinary, [string] $SkillBinary, [string] $GlobalSource, [string] $GlobalAction)
    $staged = Join-Path $script:InstallerTx 'staged'
    foreach ($name in 'work', 'tooling', 'shared') {
        Copy-Item -LiteralPath (Join-Path $Root "skills/$name") -Destination (Join-Path $staged $name) -Recurse -ErrorAction Stop
    }
    foreach ($name in 'work', 'tooling') {
        Write-InstallerSkillEntry -Root $Root -Name $name -Destination (Join-Path $staged "host-$name")
        Remove-Item -LiteralPath (Join-Path $staged "$name/SKILL.md") -ErrorAction Stop
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
    if ($entries.Count -eq 0 -or $null -eq $entries[0]) { throw 'no installation resources were prepared' }
    if (-not [string]::IsNullOrEmpty($GlobalDir)) { $entries += "$GlobalSourceSha  global/$([IO.Path]::GetFileName($GlobalTarget))" }
    return $entries
}

# Checks the installed result: exact managed content, runnable binaries and the Global Instructions copy.
function Test-InstallerInstallation {
    param([string[]] $Entries, [string] $GlobalDir, [string] $GlobalTarget, [string] $GlobalSourceSha)
    if (-not (Test-InstallerManifestMatches -InstallDir $script:InstallerInstallDir -Entries $Entries)) { return $false }
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
