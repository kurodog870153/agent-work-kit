Set-StrictMode -Version Latest

function Copy-InstallerWorkPayload {
    param([string] $SourceWork, [string] $Destination, [string] $Binary, [string] $StateJson)
    foreach ($relative in @('SKILL.md', 'agents/openai.yaml', 'references/instruction-loading.md')) {
        $to = Join-Path $Destination $relative
        New-Item -ItemType Directory -Path (Split-Path -Path $to -Parent) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $SourceWork $relative) -Destination $to -ErrorAction Stop
    }
    foreach ($relative in @('references/instruction-loading', 'references/workflows', 'references/subagents')) {
        $to = Join-Path $Destination $relative
        New-Item -ItemType Directory -Path $to -Force | Out-Null
        Copy-Item -Path (Join-Path $SourceWork "$relative/*") -Destination $to -Recurse -Force -ErrorAction Stop
    }
    foreach ($mode in @('task', 'execute')) {
        $relative = "references/instructions/$mode"
        $to = Join-Path $Destination $relative
        New-Item -ItemType Directory -Path $to -Force | Out-Null
        Copy-Item -Path (Join-Path $SourceWork "$relative/*") -Destination $to -Recurse -Force -ErrorAction Stop
    }
    $scripts = Join-Path $Destination 'scripts'
    New-Item -ItemType Directory -Path $scripts -Force | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $scripts 'work.exe') -ErrorAction Stop
    [IO.File]::WriteAllText((Join-Path $Destination 'install-state.json'), $StateJson, [Text.UTF8Encoding]::new($false))
}

function Test-InstallerWorkPayload {
    param([string] $Path, [string] $Target)
    $installed = Join-Path $Path 'scripts/work.exe'
    & $installed --help | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'installed Work binary smoke test failed' }
    if ($null -eq (Read-InstallerState -Raw ([IO.File]::ReadAllText((Join-Path $Path 'install-state.json'))) -InstallDir $Target)) {
        throw 'installed state is invalid'
    }
}
