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
foreach ($module in @('cli/parse-options.ps1', 'state/state-core.ps1', 'target/preflight.ps1', 'lock/mutex.ps1',
    'transaction/work.ps1', 'install/work-install.ps1', 'install/tooling-install.ps1', 'transaction/tooling.ps1', 'install/migration-cleanup.ps1', 'global-agents/policy.ps1', 'global-agents/transaction.ps1')) {
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
    foreach ($process in @(Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($Binary)) -ErrorAction SilentlyContinue)) {
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

function Remove-InstallerChildStates {
    param([object] $Combined, [string] $WorkTarget, [string] $ToolingTarget)
    foreach ($entry in @(
        [pscustomobject]@{ Path = (Join-Path $WorkTarget 'install-state.json'); Domain = 'work' },
        [pscustomobject]@{ Path = (Join-Path $ToolingTarget 'install-state.json'); Domain = 'tooling' }
    )) {
        if (-not (Test-Path -LiteralPath $entry.Path)) { continue }
        $item = Get-Item -LiteralPath $entry.Path -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "unsafe child install-state.json: $($entry.Path)"
        }
        $raw = [IO.File]::ReadAllText($entry.Path)
        if ($entry.Domain -eq 'work') {
            $actual = Read-InstallerState -Raw $raw -InstallDir $WorkTarget
            if ($null -eq $actual -or $actual.version -cne $Combined.work_version -or
                $actual.global_agents_dir -cne $Combined.global_agent_dir) {
                throw "child Work state differs from parent: $($entry.Path)"
            }
        } elseif ((Get-InstallerToolingStateVersion -Raw $raw -Target $ToolingTarget) -cne $Combined.tooling_version) {
            throw "child Tooling state differs from parent: $($entry.Path)"
        }
        Remove-Item -LiteralPath $entry.Path -ErrorAction Stop
    }
}

function Invoke-InstallerCore {
    $sourceWork = Join-Path $Root 'skills/work'
    $sourceTooling = Join-Path $Root 'skills/tooling'
    $stateTemplatePath = if ($Mode -eq 'source') {
        Join-Path $Root 'os-scripts/common/installer/install-state.json'
    } else {
        Join-Path $Root 'installer/state/install-state.json'
    }
    $cli = Parse-InstallerCli -Tokens $CliArgs
    if ($cli.Action -eq 'help') { Get-InstallerCliUsage; return }
    if ($cli.Verbose) { Write-Output 'Installer options parsed.' }
    if (-not (Test-InstallerToolingSource -Source $sourceTooling)) { throw 'Tooling source is missing or unsafe' }
    $install = {
    param($sourceWork, $sourceTooling, $cli, $Root, $Mode, $stateTemplatePath)
    $defaultParent = Join-Path $env:USERPROFILE '.agents/skills'
    $defaultTarget = Join-Path $defaultParent 'work'
    $parentInput = if ($null -ne $cli.SkillsInstallDir) { $cli.SkillsInstallDir } else { $defaultParent }
    $targetInput = Join-Path $parentInput 'work'
    $defaultGlobalDirectory = Join-Path $env:USERPROFILE '.codex'
    $interactiveGlobalDirectory = $null
    if (-not $cli.NonInteractive) {
        if ($null -eq $cli.GlobalAgents) {
            $initialTarget = [IO.Path]::GetFullPath($targetInput)
            $initialStatePath = Join-Path (Split-Path -Parent $initialTarget) 'install-state.json'
            $initialRaw = if ((Test-InstallerDirectoryPathSafe -Path $initialTarget) -and
                (Test-Path -LiteralPath $initialStatePath -PathType Leaf) -and
                (((Get-Item -LiteralPath $initialStatePath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0)) {
                [IO.File]::ReadAllText($initialStatePath)
            } else { '' }
            $initialCombined = if ($initialRaw) { Read-InstallerCombinedState -Raw $initialRaw `
                -WorkTarget $initialTarget -ToolingTarget (Join-Path (Split-Path -Parent $initialTarget) 'tooling') } else { $null }
            $choice = Read-InstallerChoice -Prompt 'Install Global AGENTS.md? (y/n)' -Default $(if (
                $null -eq $initialCombined -or $null -ne $initialCombined.global_agent_dir) { 'y' } else { 'n' })
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
    if ($null -eq $cli.SkillsInstallDir -and -not $cli.NonInteractive) {
        $choice = Read-InstallerChoice -Prompt "Skills directory: 1) default ($defaultParent), 2) custom" -Default '1'
        switch ($choice) {
            '1' { $parentInput = $defaultParent }
            '2' {
                $parentInput = Read-InstallerChoice -Prompt 'Custom Skills directory' -Default ''
                if ([string]::IsNullOrWhiteSpace($parentInput)) { throw 'Skills directory cannot be empty' }
            }
            default { throw 'invalid Skills directory choice' }
        }
    }
    try { $skillsParent = [IO.Path]::GetFullPath($parentInput) }
    catch { throw "invalid --skills-install-dir path <$parentInput>: $_" }
    if (-not (Test-InstallerDirectoryPathSafe -Path $skillsParent)) { throw 'unsafe Skills directory' }
    $target = Join-Path $skillsParent 'work'
    $toolingTarget = Join-Path $skillsParent 'tooling'
    $combinedPath = Join-Path $skillsParent 'install-state.json'
    $pendingStatePath = Join-Path $skillsParent '.install-state.json.tmp'
    $previousStatePath = Join-Path $skillsParent '.install-state.json.previous'
    if (Test-Path -LiteralPath $previousStatePath) {
        if (-not (Test-Path -LiteralPath $combinedPath -PathType Leaf) -or
            $null -eq (Read-InstallerCombinedState -Raw ([IO.File]::ReadAllText($combinedPath)) `
                -WorkTarget $target -ToolingTarget $toolingTarget) -or
            $null -eq (Read-InstallerCombinedState -Raw ([IO.File]::ReadAllText($previousStatePath)) `
                -WorkTarget $target -ToolingTarget $toolingTarget)) {
            throw 'previous parent state needs manual review'
        }
        Remove-Item -LiteralPath $previousStatePath -ErrorAction Stop
    }
    if (Test-Path -LiteralPath $pendingStatePath) {
        $pendingItem = Get-Item -LiteralPath $pendingStatePath -Force
        if ($pendingItem.PSIsContainer -or ($pendingItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            $null -eq (Read-InstallerCombinedState -Raw ([IO.File]::ReadAllText($pendingStatePath)) `
                -WorkTarget $target -ToolingTarget $toolingTarget)) {
            throw "unsafe pending parent state; preserve $pendingStatePath"
        }
        if (Test-Path -LiteralPath $combinedPath) {
            if ($null -eq (Read-InstallerCombinedState -Raw ([IO.File]::ReadAllText($combinedPath)) `
                -WorkTarget $target -ToolingTarget $toolingTarget)) {
                throw 'existing parent state is invalid; preserve both state files'
            }
            [IO.File]::Replace($pendingStatePath, $combinedPath, $previousStatePath)
            Remove-Item -LiteralPath $previousStatePath -ErrorAction Stop
        } else {
            Move-Item -LiteralPath $pendingStatePath -Destination $combinedPath -ErrorAction Stop
        }
    }
    $combinedRaw = if (Test-Path -LiteralPath $combinedPath -PathType Leaf) { [IO.File]::ReadAllText($combinedPath) } else { '' }
    $combinedState = if ($combinedRaw) { Read-InstallerCombinedState -Raw $combinedRaw -WorkTarget $target -ToolingTarget $toolingTarget } else { $null }
    if ($combinedRaw -and $null -eq $combinedState) { throw 'parent install-state.json is invalid; preserve the existing installation' }
    if ($null -eq $combinedState) {
        foreach ($oldTarget in @($target, $toolingTarget)) {
            if ((Test-Path -LiteralPath $oldTarget -PathType Container) -and
                @(Get-ChildItem -LiteralPath $oldTarget -Force).Count -gt 0) {
                throw "existing Skill has no parent install-state.json; preserve and inspect $oldTarget"
            }
        }
    }
    if (-not $cli.DryRun -and (Test-Path -LiteralPath (Split-Path -Path $target -Parent) -PathType Container)) {
        Invoke-InstallerMigrationCleanup -Target $target -SourceWork $sourceWork
    }
    if ($cli.DryRun) {
        if ((Test-InstallerWorkTransactionPending -Target $target) -or
            (@(Get-InstallerToolingTransactions -Target $toolingTarget).Count -gt 0) -or
            (Test-InstallerGlobalTransactionPending -WorkTarget $target)) {
            throw 'dry run cannot resolve a pending installation transaction'
        }
    } elseif (Test-Path -LiteralPath (Split-Path -Path $target -Parent) -PathType Container) {
        Restore-InstallerInterruptedGlobal -WorkTarget $target -SourceWork $sourceWork
        Restore-InstallerInterruptedWork -Target $target -SourceWork $sourceWork
    }
    $releaseInfo = if ($Mode -eq 'package') {
        Get-Content -LiteralPath (Join-Path $Root 'release-info.json') -Raw | ConvertFrom-Json
    } else { $null }
    $version = if ($Mode -eq 'source') { ([IO.File]::ReadAllText((Join-Path $Root 'VERSION'))).Trim() } else { $releaseInfo.version }
    $workVersion = if ($Mode -eq 'source') {
        $manifest = [IO.File]::ReadAllText((Join-Path $Root 'rust/Cargo.toml'))
        $match = [regex]::Match($manifest, '(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"$')
        if (-not $match.Success) { throw 'Rust workspace version is missing or invalid' }
        $match.Groups[1].Value
    } else { $releaseInfo.work_version }
    if ($workVersion -isnot [string] -or $workVersion -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
        throw 'Work version is missing or invalid'
    }
    $toolingVersion = if ($Mode -eq 'source') { $workVersion } else { $releaseInfo.tooling_version }
    if ($toolingVersion -isnot [string] -or $toolingVersion -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
        throw 'Tooling version is missing or invalid'
    }
    if ($null -ne $combinedState -and $combinedState.status -eq 'installing' -and
        ($combinedState.version -cne $version -or $combinedState.work_version -cne $workVersion -or
            $combinedState.tooling_version -cne $toolingVersion)) {
        throw 'pending installation belongs to a different release; preserve its recovery data'
    }
    $rustRoot = Join-Path $Root 'rust'
    $binary = if ($Mode -eq 'package') { Join-Path $Root 'bin/work.exe' } else { Join-Path $rustRoot 'target/release/work.exe' }
    $toolingBinary = if ($Mode -eq 'package') { Join-Path $Root 'bin/tooling.exe' } else { Join-Path $rustRoot 'target/release/tooling.exe' }
    if (-not $cli.DryRun -and $Mode -eq 'source') {
        Push-Location $rustRoot
        try {
            & cargo build --release --locked -p work-cli
            if ($LASTEXITCODE -ne 0) { throw 'Work Rust build failed' }
            & cargo build --release --locked -p tooling-cli
            if ($LASTEXITCODE -ne 0) { throw 'Tooling Rust build failed' }
        }
        finally { Pop-Location }
    }
    if (-not $cli.DryRun) {
        foreach ($candidate in @($binary, $toolingBinary)) {
            if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) { throw "release binary is missing: $candidate" }
            & $candidate --help | Out-Null
            if ($LASTEXITCODE -ne 0) { throw "release binary smoke test failed: $candidate" }
        }
    }
    if (-not $cli.DryRun -and (Test-Path -LiteralPath $skillsParent -PathType Container)) {
        Restore-InstallerInterruptedTooling -Target $toolingTarget -Source $sourceTooling -Version $toolingVersion -Binary $toolingBinary
        if ($null -ne $combinedState -and $combinedState.status -eq 'installed' -and
            -not (Test-InstallerWorkTransactionPending -Target $target) -and
            @(Get-InstallerToolingTransactions -Target $toolingTarget).Count -eq 0) {
            foreach ($childState in @((Join-Path $target 'install-state.json'), (Join-Path $toolingTarget 'install-state.json'))) {
                if (Test-Path -LiteralPath $childState) {
                    throw "existing child install-state.json is unsupported; preserve and inspect $childState"
                }
            }
        }
    }
    if ($cli.Verbose) { Write-Output "Installer: candidate version $version; Work target selected" }
    $rawState = if ($null -ne $combinedState) {
        ([ordered]@{
            version = $combinedState.work_version
            skills = @(Get-InstallerExpandedSkills -Selection 'all')
            work_dir = $target
            global_agents_dir = $combinedState.global_agent_dir
        } | ConvertTo-Json -Depth 5 -Compress)
    } else { '' }
    $settingArgs = @{ Raw = $rawState; InstallDir = $target }
    $settingArgs.SkillsOverride = 'all'
    if ($null -ne $cli.GlobalAgents) { $settingArgs.GlobalAgentsOverride = $cli.GlobalAgents }
    $settings = Resolve-InstallerSettings @settingArgs
    $targetStateValid = $settings.HasValidState
    $migration = $null
    $skills = @($settings.Skills)
    $globalEnabled = [bool] $settings.GlobalAgents
    if ($cli.Verbose) {
        $loggedState = Read-InstallerState -Raw $rawState -InstallDir $target
        $loggedVersion = if ($null -ne $loggedState) { $loggedState.version } else { 'none' }
        Write-Output "Installer: installed version $loggedVersion; selected Skills $($skills -join ',')"
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
        catch { throw "invalid --global-agent-install-dir path <$globalDirectoryInput>: $_" }
        if (-not (Test-InstallerDirectoryPathSafe -Path $globalDirectory)) { throw 'unsafe Global AGENTS directory' }
        if ($cli.Verbose) { Write-Output 'Installer: Global AGENTS directory checked' }
    }
    $inspection = Inspect-InstallerTarget -Target $target -SourceWork $sourceWork -StateValid $targetStateValid
    $toolingInspection = Inspect-InstallerToolingTarget -Target $toolingTarget -Source $sourceTooling
    $toolingConfirmed = $false
    if ($toolingInspection.Kind -eq 'first-update' -and -not $cli.NonInteractive) {
        Write-Output "Existing Tooling without valid install-state.json: $toolingTarget"
        $toolingConfirmed = (Read-InstallerChoice -Prompt 'Type YES to take over this Tooling directory' -Default 'no') -ceq 'YES'
    }
    if ($toolingInspection.Kind -notin @('new', 'managed') -and
        -not ($toolingInspection.Kind -eq 'first-update' -and $toolingConfirmed)) {
        throw "Tooling target cannot be installed: $($toolingInspection.Reason)"
    }
    $confirmed = $false
    if ($inspection.Kind -eq 'first-update' -and -not $cli.NonInteractive) {
        Write-Output "Existing Work without valid install-state.json: $target"
        Write-Output 'Required structure and managed content passed safety inspection.'
        $confirmed = (Read-InstallerChoice -Prompt 'Type YES to take over this Work directory' -Default 'no') -ceq 'YES'
    }
    if (-not (Test-InstallerTargetAuthorization -Inspection $inspection -NonInteractive $cli.NonInteractive -Confirmed $confirmed)) {
        if ($inspection.Kind -eq 'unsafe') {
            throw "Work target cannot be installed: $($inspection.Reason). Keep the existing files. Inspect and organize this directory yourself, or choose another --skills-install-dir."
        }
        throw "Work target cannot be installed: $($inspection.Reason)"
    }
    $existingState = if ($null -ne $migration) { $migration.State } else { Read-InstallerState -Raw $rawState -InstallDir $target }
    if ($null -ne $combinedState -and $combinedState.status -eq 'installed' -and
        [version]$version -lt [version]$combinedState.version) {
        throw "release downgrade from $($combinedState.version) to $version is not allowed"
    }
    if ($null -ne $existingState -and [version]$workVersion -lt [version]$existingState.version) {
        throw "downgrade from $($existingState.version) to $workVersion is not allowed"
    }
    if ($null -ne $toolingInspection.Version -and
        (Test-Path -LiteralPath (Join-Path $toolingTarget 'scripts/tooling.exe') -PathType Leaf) -and
        [version]$toolingVersion -lt [version]$toolingInspection.Version) {
        throw "Tooling downgrade from $($toolingInspection.Version) to $toolingVersion is not allowed"
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
    if (Test-InstallerBinaryInUse -Binary (Join-Path $toolingTarget 'scripts/tooling.exe')) {
        throw 'installed Tooling binary is in use; close that Tooling process and retry'
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
    Write-Output "Tooling target: $toolingTarget"
    Write-Output "Version: $version; Global AGENTS: $globalEnabled; Work action: $($inspection.Kind); Tooling action: $($toolingInspection.Kind)"
    if ($cli.DryRun) { Write-Output 'Dry run: no installation target changed.'; return }
    if (-not $cli.NonInteractive) {
        if ((Read-InstallerChoice -Prompt 'Proceed with installation? (y/n)' -Default 'n') -ne 'y') { throw 'installation cancelled' }
    }
    if ($cli.Verbose) { Write-Output 'Installer: candidate Work binary smoke test passed' }
    $stateArgs = @{
        TemplateRaw = [IO.File]::ReadAllText($stateTemplatePath)
        Version = $workVersion
        Skills = ($skills -join ',')
        GlobalAgents = $globalEnabled
        WorkDir = $target
        GlobalAgentsDir = $(if ($globalEnabled) { $globalDirectory } else { $null })
    }
    $workState = New-InstallerState @stateArgs
    $stateJson = ($workState | ConvertTo-Json -Depth 4) + "`n"
    $toolingState = [ordered]@{ version = $toolingVersion; skill = 'tooling'; tooling_dir = $toolingTarget }
    $toolingStateJson = ($toolingState | ConvertTo-Json -Compress) + "`n"
    $prepare = {
        param($prepared)
        Copy-InstallerWorkPayload -SourceWork $sourceWork -Destination $prepared -Binary $binary -StateJson $stateJson
    }
    $validate = {
        param($path)
        Test-InstallerWorkPayload -Path $path -Target $target
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
        Write-InstallerCombinedState -Parent $skillsParent -Version $version -WorkVersion $workVersion `
            -ToolingVersion $toolingVersion -GlobalAgentDir $(if ($globalEnabled) { $globalDirectory } else { $null }) `
            -Status 'installing'
        $result = Invoke-InstallerCombinedTransaction -Target $target -SourceWork $sourceWork -Prepare $prepare -Validate $validate -StateValid $targetStateValid -Confirmed $confirmed -NonInteractive $cli.NonInteractive -GlobalPlan $plan -GlobalSource $globalSource -GlobalTarget $globalTarget
        if ($cli.Verbose) { Write-Output 'Installer: Work transaction published and verified' }
        Write-Output "Work installed at $result"
        try {
            $toolingResult = Invoke-InstallerToolingTransaction -Target $toolingTarget -Source $sourceTooling -Version $toolingVersion -Binary $toolingBinary -StateJson $toolingStateJson -NonInteractive $cli.NonInteractive -Confirmed $toolingConfirmed
        } catch {
            throw "Tooling installation failed after Work was published; inspect $toolingTarget and retry for recovery. $_"
        }
        Write-Output "Tooling installed at $toolingResult"
        if ((Test-InstallerWorkTransactionPending -Target $target) -or
            @(Get-InstallerToolingTransactions -Target $toolingTarget).Count -gt 0) {
            throw 'installation transaction cleanup is pending; preserve its recovery data'
        }
        Write-InstallerCombinedState -Parent $skillsParent -Version $version -WorkVersion $workVersion `
            -ToolingVersion $toolingVersion -GlobalAgentDir $(if ($globalEnabled) { $globalDirectory } else { $null }) `
            -Status 'installed'
        $finalState = Read-InstallerCombinedState -Raw ([IO.File]::ReadAllText($combinedPath)) `
            -WorkTarget $target -ToolingTarget $toolingTarget
        if ($null -eq $finalState) { throw 'published parent install-state.json is invalid' }
        Remove-InstallerChildStates -Combined $finalState -WorkTarget $target -ToolingTarget $toolingTarget
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
    Invoke-InstallerLocked -Transaction $install -Arguments @($sourceWork, $sourceTooling, $cli, $Root, $Mode, $stateTemplatePath)
}

try {
    Invoke-InstallerCore
    exit 0
} catch {
    [Console]::Error.WriteLine("Error: $($_.Exception.Message)")
    if ($_.Exception.Data.Contains('ExitCode')) { exit [int]$_.Exception.Data['ExitCode'] }
    exit 1
}
