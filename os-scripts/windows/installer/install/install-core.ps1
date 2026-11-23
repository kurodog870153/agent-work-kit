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
foreach ($module in @('cli/parse-options.ps1', 'state/state.ps1', 'target/preflight.ps1', 'lock/mutex.ps1', 'install/resources.ps1',
    'global-instructions/policy.ps1', 'transaction/transaction.ps1')) {
    . (Join-Path $installerRoot $module)
}

function Read-InstallerChoice {
    param([string] $Prompt, [string] $Default)
    $answer = Read-Host "$Prompt [$Default]"
    if ([string]::IsNullOrWhiteSpace($answer)) { return $Default }
    return $answer.Trim()
}

function Invoke-InstallerCore {
    $cli = Parse-InstallerCli -Tokens $CliArgs
    if ($cli.Action -eq 'help') { Get-InstallerCliUsage; return }
    function Write-InstallerLog([string] $Message) { if ($cli.Verbose) { Write-Output "Installer: $Message" } }
    $selectedHost = $cli.InstallerHost
    if ($null -eq $selectedHost) {
        switch (Read-InstallerChoice -Prompt 'Select Host: 1) Codex, 2) Claude Code' -Default '') {
            '1' { $selectedHost = 'codex' }
            '2' { $selectedHost = 'claude' }
            default { throw 'Host selection is required' }
        }
    }

    # --- Release identity ---
    if ($Mode -eq 'package') {
        $info = Join-Path $Root 'release-info.json'
        $packageHost = Get-InstallerReleaseField -Path $info -Name 'host'
        $packageTarget = Get-InstallerReleaseField -Path $info -Name 'target'
        $version = Get-InstallerReleaseField -Path $info -Name 'version'
        $rustVersion = Get-InstallerReleaseField -Path $info -Name 'rust_workspace_version'
        if ($packageHost -cne $selectedHost -or $packageTarget -cne 'x86_64-pc-windows-msvc') {
            throw 'release package Host or platform does not match the selected installation'
        }
        $workBinary = Join-Path $Root 'bin/work.exe'
        $skillBinary = Join-Path $Root 'bin/skill.exe'
    } else {
        $version = ([IO.File]::ReadAllText((Join-Path $Root 'VERSION'))).Trim()
        $match = [regex]::Match([IO.File]::ReadAllText((Join-Path $Root 'rust/Cargo.toml')), '(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"\r?$')
        if (-not $match.Success) { throw 'Rust workspace version is missing or invalid' }
        $rustVersion = $match.Groups[1].Value
        $workBinary = Join-Path $Root 'rust/target/release/work.exe'
        $skillBinary = Join-Path $Root 'rust/target/release/skill.exe'
    }
    if (-not (Test-InstallerVersion $version) -or -not (Test-InstallerVersion $rustVersion)) { throw 'release or Rust workspace version is missing or invalid' }
    Assert-InstallerSources -Root $Root
    $globalSource = Join-Path $Root 'instructions/global-instructions.md'

    # --- Locations ---
    $hostHome = if ($selectedHost -eq 'codex') { Join-Path $env:USERPROFILE '.agents' } else { Join-Path $env:USERPROFILE '.claude' }
    $defaultGlobalDir = if ($selectedHost -eq 'codex') { Join-Path $env:USERPROFILE '.codex' } else { Join-Path $env:USERPROFILE '.claude' }
    $globalFilename = if ($selectedHost -eq 'codex') { 'AGENTS.md' } else { 'CLAUDE.md' }
    $hostSkillsDir = Join-Path $hostHome 'skills'
    $installInput = if ($null -ne $cli.InstallDir) { $cli.InstallDir } else { Join-Path $hostHome 'agent-work-kit' }
    if ($null -eq $cli.InstallDir -and -not $cli.NonInteractive) {
        switch (Read-InstallerChoice -Prompt "Installation directory: 1) default ($installInput), 2) custom" -Default '1') {
            '1' { }
            '2' {
                $installInput = Read-InstallerChoice -Prompt 'Custom installation directory' -Default ''
                if ([string]::IsNullOrWhiteSpace($installInput)) { throw 'Installation directory cannot be empty' }
            }
            default { throw 'invalid installation directory choice' }
        }
    }
    try { $installDir = [IO.Path]::GetFullPath($installInput).TrimEnd('\', '/') } catch { throw "invalid --install-dir path <$installInput>: $_" }
    if (-not (Test-InstallerDirectoryPathSafe -Path $installDir)) { throw 'unsafe installation directory' }
    if (-not (Test-InstallerDirectoryPathSafe -Path $hostSkillsDir)) { throw 'unsafe Host Skills directory' }
    if (Test-InstallerPathOverlap $installDir $hostSkillsDir) { throw 'installation directory must not overlap the Host Skills directory' }
    $skillsDir = Join-Path $installDir 'skills'
    $cliDir = $installDir
    $script:InstallerInstallDir = $installDir
    $script:InstallerHostSkillsDir = $hostSkillsDir
    $script:InstallerSkillsDir = $skillsDir
    $script:InstallerCliDir = $cliDir
    $script:InstallerTx = Join-Path $installDir '.agent-work-kit-txn'
    $script:InstallerCreatedDirs = @()
    $script:InstallerGlobalTarget = ''

    # --- Recovery and target inspection ---
    if (Test-InstallerExists $script:InstallerTx) {
        if ($cli.DryRun) { throw 'dry run cannot resolve a pending installation transaction' }
        Restore-InstallerInterrupted
    }
    $inspection = Get-InstallerTargetInspection -InstallDir $installDir -HostSkillsDir $hostSkillsDir -InstallerHost $selectedHost
    if ($inspection.Kind -eq 'unsafe') {
        throw "targets cannot be installed: $($inspection.Reason). Existing content is preserved and not taken over; inspect it yourself or choose other directories."
    }
    $previousGlobalDir = ''
    $previousGlobalSha = ''
    if ($inspection.Kind -eq 'managed' -and $null -ne $inspection.State.global_instructions_dir) {
        $previousGlobalDir = [string] $inspection.State.global_instructions_dir
        foreach ($entry in @($inspection.State.files)) { if ($entry -cmatch '^([0-9a-f]{64})  global/') { $previousGlobalSha = $Matches[1] } }
    }

    # --- Global Instructions ---
    $globalEnabled = ($inspection.Kind -eq 'new') -or ($previousGlobalDir -ne '')
    if ($null -ne $cli.GlobalInstructions) { $globalEnabled = [bool] $cli.GlobalInstructions }
    elseif (-not $cli.NonInteractive) {
        $choice = Read-InstallerChoice -Prompt 'Install Global Instructions? (y/n)' -Default $(if ($globalEnabled) { 'y' } else { 'n' })
        if ($choice -notin @('y', 'n')) { throw 'invalid Global Instructions choice' }
        $globalEnabled = $choice -eq 'y'
    }
    $globalDir = ''
    if ($globalEnabled) {
        $globalInput = if ($previousGlobalDir -ne '') { $previousGlobalDir } else { $defaultGlobalDir }
        if ($null -ne $cli.InstructionsDir) { $globalInput = $cli.InstructionsDir }
        elseif (-not $cli.NonInteractive) {
            switch (Read-InstallerChoice -Prompt "Global Instructions directory: 1) default ($globalInput), 2) custom" -Default '1') {
                '1' { }
                '2' {
                    $globalInput = Read-InstallerChoice -Prompt 'Custom Global Instructions directory' -Default ''
                    if ([string]::IsNullOrWhiteSpace($globalInput)) { throw 'Global Instructions directory cannot be empty' }
                }
                default { throw 'invalid Global Instructions directory choice' }
            }
        }
        try { $globalDir = [IO.Path]::GetFullPath($globalInput).TrimEnd('\', '/') } catch { throw "invalid --instructions-dir path <$globalInput>: $_" }
        if (-not (Test-InstallerDirectoryPathSafe -Path $globalDir)) { throw 'unsafe Global Instructions directory' }
        $script:InstallerGlobalTarget = Join-Path $globalDir $globalFilename
        if ((Test-InstallerPathOverlap $script:InstallerGlobalTarget $skillsDir) -or (Test-InstallerPathOverlap $script:InstallerGlobalTarget $cliDir)) {
            throw 'Global Instructions must be outside the Skills and CLI directories'
        }
        if (-not (Test-InstallerPathEqual $globalDir $previousGlobalDir)) { $previousGlobalSha = '' }
    }
    try {
        $plan = Get-InstallerGlobalPlan -Source $globalSource -Target $script:InstallerGlobalTarget -Enabled $globalEnabled `
            -NonInteractive $cli.NonInteractive -PreviousSha $previousGlobalSha
    } catch {
        throw "Global Instructions target differs, is unsafe, or needs interactive confirmation (use --global-instructions no to skip it): $($_.Exception.Message)"
    }
    $globalAction = $plan.Action
    if ($globalAction -eq 'confirm') {
        Write-Output "Global $globalFilename differs (current SHA-256 $($plan.TargetSha256), new $($plan.SourceSha256))."
        if ((Read-InstallerChoice -Prompt "Replace Global $globalFilename? (y/n)" -Default 'n') -eq 'y') { $globalAction = 'replace' }
        else { $globalAction = 'skip'; $globalDir = ''; $script:InstallerGlobalTarget = '' }
    }

    Write-InstallerLog 'locations selected'

    # --- Binaries ---
    if (-not $cli.DryRun) {
        if ($Mode -eq 'source') {
            Push-Location (Join-Path $Root 'rust')
            try {
                & cargo build --release --locked -p work-cli -p skill-cli
                if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
            } finally { Pop-Location }
        }
        foreach ($binary in @($workBinary, $skillBinary)) {
            if (-not (Test-Path -LiteralPath $binary -PathType Leaf) -or (Test-InstallerReparse (Get-Item -LiteralPath $binary -Force))) { throw "release binary is missing: $binary" }
            & $binary --help *> $null
            if ($LASTEXITCODE -ne 0) { throw "release binary failed its startup check: $binary" }
        }
    }
    foreach ($binary in 'work', 'skill') {
        while (Test-InstallerBinaryInUse -Binary (Join-Path $cliDir "bin/$binary.exe")) {
            if ($cli.NonInteractive) { throw "installed $binary binary is in use; close it and retry" }
            if ((Read-InstallerChoice -Prompt "Installed $binary is in use. Close it, then choose r) recheck or c) cancel" -Default 'c') -ne 'r') { throw 'installation cancelled' }
        }
    }

    # --- Plan and confirmation ---
    Write-Output "Host: $selectedHost; version: $version; Rust workspace: $rustVersion; action: $($inspection.Kind)"
    Write-Output "Install: $installDir"
    Write-Output "Skills: $skillsDir\{work,tooling,shared}"
    Write-Output "Host discovery: $hostSkillsDir\{work,tooling}"
    Write-Output "CLI: $cliDir\bin\{work,skill}"
    Write-Output "Global Instructions: $(if ($script:InstallerGlobalTarget) { $script:InstallerGlobalTarget } else { 'disabled' }) ($globalAction)"
    if ($cli.DryRun) { Write-Output 'Dry run: no installation target changed.'; return }
    if (-not $cli.NonInteractive -and (Read-InstallerChoice -Prompt 'Proceed with installation? (y/n)' -Default 'n') -ne 'y') { throw 'installation cancelled' }

    # --- Transaction: Prepare, Apply, Verify, Commit ---
    $script:InstallerCreatedDirs = @()
    $started = $false
    try {
        New-InstallerDirectories -Path $installDir
        Start-InstallerTx
        $started = $true
        New-InstallerDirectories -Path $skillsDir
        New-InstallerDirectories -Path $hostSkillsDir
        if ($globalDir -ne '') { New-InstallerDirectories -Path $globalDir }
        Add-InstallerTxCreated -Directories $script:InstallerCreatedDirs -Reset
        Invoke-InstallerInject 'prepare'
        Copy-InstallerPayload -Root $Root -WorkBinary $workBinary -SkillBinary $skillBinary -GlobalSource $globalSource -GlobalAction $globalAction
        $entries = Get-InstallerNewManifest -GlobalDir $globalDir -GlobalTarget $script:InstallerGlobalTarget -GlobalSourceSha $plan.SourceSha256
        $statePath = Join-Path $script:InstallerTx 'staged/state'
        Write-InstallerState -Path $statePath -InstallerHost $selectedHost -Version $version -RustVersion $rustVersion `
            -InstallDir $installDir -GlobalDir $globalDir -Files $entries
        if ($null -eq (Read-InstallerState -Path $statePath)) { throw 'prepared install state is invalid' }
        $components = @('work', 'tooling', 'shared', 'bin')
        if ($inspection.Kind -eq 'new') { $components += @('link-work', 'link-tooling') }
        if ($globalAction -in @('install', 'replace')) { $components += 'global' }
        Write-InstallerTxPlan -Names ($components + 'state')
        foreach ($component in $components) { Invoke-InstallerTxApply -Name $component }
        Write-InstallerLog 'prepared and applied'
        Invoke-InstallerInject 'verify'
        if (-not (Test-InstallerInstallation -Entries $entries -GlobalDir $globalDir -GlobalTarget $script:InstallerGlobalTarget -GlobalSourceSha $plan.SourceSha256)) {
            throw 'installed result failed verification'
        }
        Invoke-InstallerInject 'commit'
        [IO.File]::WriteAllText((Join-Path $script:InstallerTx 'committed'), '', [Text.UTF8Encoding]::new($false))
    } catch {
        $reason = $_.Exception.Message
        if ($started) {
            try { Undo-InstallerTx; [Console]::Error.WriteLine('The previous installation state was restored.') }
            catch { [Console]::Error.WriteLine("Recovery is incomplete ($($_.Exception.Message)); the next installer run retries it.") }
        } else { Remove-InstallerDirectories -Directories $script:InstallerCreatedDirs }
        throw $reason
    }
    Invoke-InstallerInject 'after-commit'
    try { Complete-InstallerTx } catch { throw "installation is committed; rerun the installer to finish cleanup: $($_.Exception.Message)" }
    Write-Output "Installed Work, Tooling, Shared and the Work/Skill CLI for $selectedHost."
}

try {
    Invoke-InstallerLocked -Transaction { Invoke-InstallerCore }
    exit 0
} catch {
    [Console]::Error.WriteLine("Error: $($_.Exception.Message)")
    if ($_.Exception.Data.Contains('ExitCode')) { exit [int]$_.Exception.Data['ExitCode'] }
    exit 1
}
