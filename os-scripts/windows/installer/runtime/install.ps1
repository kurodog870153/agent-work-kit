$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($args.Count -lt 4 -or $args[0] -cne '-Mode' -or $args[2] -cne '-Root' -or
    $args[1] -cnotin @('source', 'package') -or [string]::IsNullOrEmpty($args[3])) {
    [Console]::Error.WriteLine('Error: installer core requires -Mode source|package -Root PATH.')
    exit 2
}
$Mode = [string] $args[1]
$Root = [string] $args[3]
$CliArgs = @()
if ($args.Count -gt 4) { $CliArgs = [string[]] $args[4..($args.Count - 1)] }
$installerRoot = Split-Path -Parent $PSScriptRoot
foreach ($module in @('cli/parse-options.ps1', 'state/state-model.ps1', 'target/preflight.ps1', 'lock/mutex.ps1',
    'transaction/work.ps1', 'runtime/migration-cleanup.ps1', 'global-agents/policy.ps1', 'global-agents/transaction.ps1')) {
    . (Join-Path $installerRoot $module)
}

function Read-InstallerChoice {
    param([string] $Prompt, [string] $Default)
    $answer = Read-Host "$Prompt [$Default]"
    if ([string]::IsNullOrWhiteSpace($answer)) { return $Default }
    return $answer.Trim()
}

function Test-InstallerDirectoryPathSafe {
    param([string] $Path)
    $probe = $Path
    while (-not [string]::IsNullOrEmpty($probe)) {
        if (Test-Path -LiteralPath $probe) {
            $item = Get-Item -LiteralPath $probe -Force
            if (-not $item.PSIsContainer -or
                ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
        }
        $parent = Split-Path -Path $probe -Parent
        if ($parent -eq $probe) { break }
        $probe = $parent
    }
    return $true
}

function Copy-InstallerWorkPayload {
    param([string] $SourceWork, [string] $Destination, [string[]] $Skills, [string] $Binary, [string] $StateJson)
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
    $paths = Get-InstallerSkillPaths
    foreach ($id in $Skills) {
        $hierarchy = $paths[$id]
        foreach ($mode in @('task', 'execute')) {
            $relative = "references/instructions/$mode/$hierarchy"
            $to = Join-Path $Destination $relative
            New-Item -ItemType Directory -Path $to -Force | Out-Null
            Copy-Item -Path (Join-Path $SourceWork "$relative/*") -Destination $to -Recurse -Force -ErrorAction Stop
        }
    }
    $scripts = Join-Path $Destination 'scripts'
    New-Item -ItemType Directory -Path $scripts -Force | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $scripts 'work.exe') -ErrorAction Stop
    [IO.File]::WriteAllText((Join-Path $Destination 'install-state.json'), $StateJson, [Text.UTF8Encoding]::new($false))
}

function New-InstallerWorkParents {
    param([string] $Target)
    $missing = @()
    $probe = Split-Path -Path $Target -Parent
    while (-not (Test-Path -LiteralPath $probe)) {
        $missing += $probe
        $next = Split-Path -Path $probe -Parent
        if ($next -eq $probe -or [string]::IsNullOrEmpty($next)) { throw 'cannot resolve Work parent' }
        $probe = $next
    }
    if (-not (Test-Path -LiteralPath $probe -PathType Container)) { throw 'Work parent is not a directory' }
    $created = @()
    for ($index = $missing.Count - 1; $index -ge 0; $index--) {
        $path = $missing[$index]
        New-Item -ItemType Directory -Path $path -ErrorAction Stop | Out-Null
        $created += $path
    }
    return $created
}

function Test-InstallerBinaryInUse {
    param([string] $Binary)
    if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { return $false }
    $full = [IO.Path]::GetFullPath($Binary)
    foreach ($process in @(Get-Process -Name work -ErrorAction SilentlyContinue)) {
        try {
            if ([string]::Equals($process.Path, $full, [StringComparison]::OrdinalIgnoreCase)) { return $true }
        } catch { }
    }
    try {
        $stream = [IO.File]::Open($full, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
        $stream.Dispose()
        return $false
    } catch [IO.IOException] {
        return $true
    }
}

function Invoke-InstallerCore {
    $sourceWork = Join-Path $Root 'skills/work'
    $stateTemplatePath = if ($Mode -eq 'source') {
        Join-Path $Root 'os-scripts/common/installer/install-state.json'
    } else {
        Join-Path $Root 'installer/state/install-state.json'
    }
    $cli = Parse-InstallerCli -Tokens $CliArgs -SourceWork $sourceWork
    if ($cli.Action -eq 'help') { Get-InstallerCliUsage; return }
    if ($cli.Action -eq 'list-skills') { Get-InstallerAvailableSkills -SourceWork $sourceWork; return }
    if ($cli.Verbose) { Write-Output 'Installer options parsed.' }
    $install = {
    param($sourceWork, $cli, $Root, $Mode, $stateTemplatePath)
    $defaultTarget = Join-Path $env:USERPROFILE '.agents/skills/work'
    $targetInput = if ($null -ne $cli.WorkInstallDir) { $cli.WorkInstallDir } else { $defaultTarget }
    $defaultGlobalDirectory = Join-Path $env:USERPROFILE '.codex'
    $interactiveGlobalDirectory = $null
    if (-not $cli.NonInteractive) {
        if ($null -eq $cli.GlobalAgents) {
            $initialTarget = [IO.Path]::GetFullPath($targetInput)
            $initialStatePath = Join-Path $initialTarget 'install-state.json'
            $initialRaw = if ((Test-InstallerDirectoryPathSafe -Path $initialTarget) -and
                (Test-Path -LiteralPath $initialStatePath -PathType Leaf) -and
                (((Get-Item -LiteralPath $initialStatePath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0)) {
                [IO.File]::ReadAllText($initialStatePath)
            } else { '' }
            $initialSettings = Resolve-InstallerSettings -Raw $initialRaw -InstallDir $initialTarget
            $choice = Read-InstallerChoice -Prompt 'Install Global AGENTS.md? (y/n)' -Default $(if ($initialSettings.GlobalAgents) { 'y' } else { 'n' })
            if ($choice -notin @('y', 'n')) { throw 'invalid Global AGENTS selection' }
            $cli.GlobalAgents = $choice -eq 'y'
        }
        if ($cli.GlobalAgents -and $null -eq $cli.GlobalAgentsInstallDir) {
            $choice = Read-InstallerChoice -Prompt "Global AGENTS directory: 1) default ($defaultGlobalDirectory), 2) custom" -Default '1'
            switch ($choice) {
                '1' { $interactiveGlobalDirectory = $defaultGlobalDirectory }
                '2' {
                    $interactiveGlobalDirectory = Read-InstallerChoice -Prompt 'Custom Global AGENTS directory' -Default ''
                    if ([string]::IsNullOrWhiteSpace($interactiveGlobalDirectory)) { throw 'Global AGENTS directory cannot be empty' }
                }
                default { throw 'invalid Global AGENTS directory choice' }
            }
        }
    }
    if ($null -eq $cli.WorkInstallDir -and -not $cli.NonInteractive) {
        $choice = Read-InstallerChoice -Prompt "Work directory: 1) default ($defaultTarget), 2) custom" -Default '1'
        switch ($choice) {
            '1' { $targetInput = $defaultTarget }
            '2' {
                $targetInput = Read-InstallerChoice -Prompt 'Custom Work directory' -Default ''
                if ([string]::IsNullOrWhiteSpace($targetInput)) { throw 'Work directory cannot be empty' }
            }
            default { throw 'invalid Work directory choice' }
        }
    }
    try { $target = [IO.Path]::GetFullPath($targetInput) }
    catch { throw "invalid --work-install-dir path <$targetInput>: $_" }
    if ([IO.Path]::GetFileName($target) -cne 'work') { throw '--work-install-dir must name a work directory' }
    if (-not $cli.DryRun -and (Test-Path -LiteralPath (Split-Path -Path $target -Parent) -PathType Container)) {
        Invoke-InstallerMigrationCleanup -Target $target -SourceWork $sourceWork
    }
    if ($cli.DryRun) {
        if ((Test-InstallerWorkTransactionPending -Target $target) -or
            (Test-InstallerGlobalTransactionPending -WorkTarget $target)) {
            throw 'dry run cannot resolve a pending installation transaction'
        }
    } elseif (Test-Path -LiteralPath (Split-Path -Path $target -Parent) -PathType Container) {
        Restore-InstallerInterruptedGlobal -WorkTarget $target -SourceWork $sourceWork
        Restore-InstallerInterruptedWork -Target $target -SourceWork $sourceWork
    }
    $version = if ($Mode -eq 'source') { ([IO.File]::ReadAllText((Join-Path $Root 'VERSION'))).Trim() } else {
        (Get-Content -LiteralPath (Join-Path $Root 'release-info.json') -Raw | ConvertFrom-Json).version
    }
    if ($cli.Verbose) { Write-Output "Installer: candidate version $version; Work target selected" }
    $statePath = Join-Path $target 'install-state.json'
    $rawState = if (Test-Path -LiteralPath $statePath -PathType Leaf) { [IO.File]::ReadAllText($statePath) } else { '' }
    $settingArgs = @{ Raw = $rawState; InstallDir = $target }
    if ($null -ne $cli.Skills) { $settingArgs.SkillsOverride = $cli.Skills }
    if ($null -ne $cli.GlobalAgents) { $settingArgs.GlobalAgentsOverride = $cli.GlobalAgents }
    $settings = Resolve-InstallerSettings @settingArgs
    $targetStateValid = $settings.HasValidState
    $migration = $null
    if (-not $settings.HasValidState -and
        -not [string]::Equals($target, $defaultTarget, [StringComparison]::OrdinalIgnoreCase) -and
        (Test-Path -LiteralPath (Join-Path $defaultTarget 'install-state.json') -PathType Leaf)) {
        $oldRaw = [IO.File]::ReadAllText((Join-Path $defaultTarget 'install-state.json'))
        $oldState = Read-InstallerState -Raw $oldRaw -InstallDir $defaultTarget
        if ($null -ne $oldState) {
            $oldInspection = Inspect-InstallerTarget -Target $defaultTarget -SourceWork $sourceWork -StateValid $true
            if ($oldInspection.Kind -eq 'managed') {
                $oldArgs = @{ Raw = $oldRaw; InstallDir = $defaultTarget }
                if ($null -ne $cli.Skills) { $oldArgs.SkillsOverride = $cli.Skills }
                if ($null -ne $cli.GlobalAgents) { $oldArgs.GlobalAgentsOverride = $cli.GlobalAgents }
                $settings = Resolve-InstallerSettings @oldArgs
                $migration = [pscustomobject]@{ Path = $defaultTarget; State = $oldState }
            }
        }
    }
    $skills = @($settings.Skills)
    $globalEnabled = [bool] $settings.GlobalAgents
    if ($cli.Verbose) {
        $loggedState = Read-InstallerState -Raw $rawState -InstallDir $target
        $loggedVersion = if ($null -ne $loggedState) { $loggedState.version } else { 'none' }
        Write-Output "Installer: installed version $loggedVersion; selected Skills $($skills -join ',')"
    }
    if (-not $cli.NonInteractive) {
        if ($null -eq $cli.Skills) {
            Write-Output "Available Skills: $(@(Get-InstallerAvailableSkills -SourceWork $sourceWork) -join ',')"
            $selection = Read-InstallerChoice -Prompt 'Skills (general, all, or comma-separated IDs)' -Default ($skills -join ',')
            if (-not (Test-InstallerSkillSelection -Value $selection -SourceWork $sourceWork)) { throw 'invalid interactive Skill selection' }
            $skills = @(Get-InstallerExpandedSkills -Selection $selection)
        }
    }
    $globalDirectoryInput = if ($null -ne $cli.GlobalAgentsInstallDir) {
        $cli.GlobalAgentsInstallDir
    } elseif ($globalEnabled -and $null -ne $settings.GlobalAgentsDir) {
        $settings.GlobalAgentsDir
    } else { $defaultGlobalDirectory }
    if ($null -ne $interactiveGlobalDirectory) { $globalDirectoryInput = $interactiveGlobalDirectory }
    $globalDirectory = $globalDirectoryInput
    if ($globalEnabled) {
        try { $globalDirectory = [IO.Path]::GetFullPath($globalDirectoryInput) }
        catch { throw "invalid --global-agents-install-dir path <$globalDirectoryInput>: $_" }
        if (-not (Test-InstallerDirectoryPathSafe -Path $globalDirectory)) { throw 'unsafe Global AGENTS directory' }
        if ($cli.Verbose) { Write-Output 'Installer: Global AGENTS directory checked' }
    }
    $inspection = Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $targetStateValid
    $confirmed = $false
    if ($inspection.Kind -eq 'first-update' -and -not $cli.NonInteractive) {
        Write-Output "Existing Work without valid install-state.json: $target"
        Write-Output 'Required structure and managed content passed safety inspection.'
        $confirmed = (Read-InstallerChoice -Prompt 'Type YES to take over this Work directory' -Default 'no') -ceq 'YES'
    }
    if (-not (Test-InstallerTargetAuthorization -Inspection $inspection -NonInteractive $cli.NonInteractive -Confirmed $confirmed)) {
        if ($inspection.Kind -eq 'unsafe') {
            throw "Work target cannot be installed: $($inspection.Reason). Keep the existing files. Inspect and organize this directory yourself, or choose another --work-install-dir."
        }
        throw "Work target cannot be installed: $($inspection.Reason)"
    }
    $existingState = if ($null -ne $migration) { $migration.State } else { Read-InstallerState -Raw $rawState -InstallDir $target }
    if ($null -ne $existingState -and [version]$version -lt [version]$existingState.version) {
        throw "downgrade from $($existingState.version) to $version is not allowed"
    }
    if ($cli.Verbose) { Write-Output 'Installer: version comparison passed' }
    while (Test-InstallerBinaryInUse -Binary (Join-Path $target 'scripts/work.exe')) {
        if ($cli.NonInteractive) { throw 'installed Work binary is in use; close that Work process and retry' }
        $choice = Read-InstallerChoice -Prompt 'Installed Work is in use. Close it, then choose r) recheck or c) cancel' -Default 'c'
        switch ($choice) {
            'r' { }
            'c' { throw 'installation cancelled' }
            default { throw 'invalid in-use Work choice' }
        }
    }
    $globalSource = Join-Path $Root 'AGENTS.md'
    $globalTarget = Join-Path $globalDirectory 'AGENTS.md'
    $plan = Get-InstallerGlobalAgentsPlan -Source $globalSource -Target $globalTarget -Enabled $globalEnabled -NonInteractive $cli.NonInteractive -Confirmed $false
    if ($plan.Action -eq 'confirm') {
        Write-Output "Global AGENTS.md differs (current SHA-256 $($plan.TargetSha256), new $($plan.SourceSha256))."
        $replace = (Read-InstallerChoice -Prompt 'Replace Global AGENTS.md? (y/n)' -Default 'n') -eq 'y'
        $plan = Get-InstallerGlobalAgentsPlan -Source $globalSource -Target $globalTarget -Enabled $globalEnabled -NonInteractive $false -Confirmed $replace
        if ($plan.Action -eq 'confirm') { $plan.Action = 'skip' }
    }
    Write-Output "Work target: $target"
    Write-Output "Version: $version; Skills: $($skills -join ','); Global AGENTS: $globalEnabled; Action: $($inspection.Kind)"
    if ($cli.DryRun) { Write-Output 'Dry run: no installation target changed.'; return }
    if (-not $cli.NonInteractive) {
        if ((Read-InstallerChoice -Prompt 'Proceed with installation? (y/n)' -Default 'n') -ne 'y') { throw 'installation cancelled' }
    }
    $binary = if ($Mode -eq 'package') { Join-Path $Root 'bin/work.exe' } else {
        $rustRoot = Join-Path $Root 'rust'
        Push-Location $rustRoot
        try { & cargo build --release --locked -p work-cli; if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' } }
        finally { Pop-Location }
        Join-Path $rustRoot 'target/release/work.exe'
    }
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Work binary is missing: $binary" }
    & $binary --help | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Work binary smoke test failed' }
    if ($cli.Verbose) { Write-Output 'Installer: candidate Work binary smoke test passed' }
    $stateArgs = @{
        TemplateRaw = [IO.File]::ReadAllText($stateTemplatePath)
        Version = $version
        Skills = ($skills -join ',')
        GlobalAgents = $globalEnabled
        WorkDir = $target
        GlobalAgentsDir = $(if ($globalEnabled) { $globalDirectory } else { $null })
    }
    $stateJson = (New-InstallerState @stateArgs | ConvertTo-Json -Depth 4) + "`n"
    $prepare = {
        param($prepared)
        Copy-InstallerWorkPayload -SourceWork $sourceWork -Destination $prepared -Skills $skills -Binary $binary -StateJson $stateJson
    }
    $validate = {
        param($path)
        $installed = Join-Path $path 'scripts/work.exe'
        & $installed --help | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'installed Work binary smoke test failed' }
        if ($null -eq (Read-InstallerState -Raw ([IO.File]::ReadAllText((Join-Path $path 'install-state.json'))) -InstallDir $target)) {
            throw 'installed state is invalid'
        }
    }
    $globalParent = Split-Path -Path $globalTarget -Parent
    $createdGlobalParent = $false
    $createdWorkParents = @(New-InstallerWorkParents -Target $target)
    if ($null -ne $migration) {
        Write-InstallerMigrationRecord -Target $target -Old $migration.Path -SourceWork $sourceWork -StateJson $stateJson
    }
    if ($plan.Action -eq 'install' -and -not (Test-Path -LiteralPath $globalParent)) {
        New-Item -ItemType Directory -Path $globalParent -ErrorAction Stop | Out-Null
        $createdGlobalParent = $true
    }
    try {
        $result = Invoke-InstallerCombinedTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -StateValid $targetStateValid -Confirmed $confirmed -NonInteractive $cli.NonInteractive -GlobalPlan $plan -GlobalSource $globalSource -GlobalTarget $globalTarget
        if ($cli.Verbose) { Write-Output 'Installer: Work transaction published and verified' }
        Write-Output "Work installed at $result"
        if ($null -ne $migration) {
            Invoke-InstallerMigrationCleanup -Target $target -SourceWork $sourceWork
        }
    } catch {
        for ($index = $createdWorkParents.Count - 1; $index -ge 0; $index--) {
            $created = $createdWorkParents[$index]
            if ((Test-Path -LiteralPath $created -PathType Container) -and
                @(Get-ChildItem -LiteralPath $created -Force).Count -eq 0) {
                Remove-Item -LiteralPath $created -ErrorAction Stop
            }
        }
        if ($createdGlobalParent -and @(Get-ChildItem -LiteralPath $globalParent -Force).Count -eq 0) {
            Remove-Item -LiteralPath $globalParent -ErrorAction Stop
        }
        throw
    }
    }
    Invoke-InstallerLocked -Transaction $install -Arguments @($sourceWork, $cli, $Root, $Mode, $stateTemplatePath)
}

try {
    Invoke-InstallerCore
    exit 0
} catch {
    [Console]::Error.WriteLine("Error: $($_.Exception.Message)")
    if ($_.Exception.Data.Contains('ExitCode')) { exit [int]$_.Exception.Data['ExitCode'] }
    exit 1
}
