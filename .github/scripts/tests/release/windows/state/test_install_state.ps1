$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/state/state-core.ps1')

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw "FAIL: $Message" }
}

$installDir = 'C:\目錄 有空白\skills\work'
$globalDir = 'C:\全域 有空白'
$template = [IO.File]::ReadAllText((Join-Path $repoRoot 'os-scripts/common/installer/install-state.json'))
$templateKeys = @((ConvertFrom-Json $template).PSObject.Properties.Name | Sort-Object -CaseSensitive) -join ','
Assert-True ($templateKeys -ceq 'global_instructions_dir,host,rust_workspace_version,skill_install_dir,status,version') 'combined state template shape'
$combinedRaw = '{"host":"codex","version":"1.2.3","rust_workspace_version":"1.2.3","skill_install_dir":"C:\\目錄 有空白\\skills","global_instructions_dir":null,"status":"installed"}'
$combined = Read-InstallerCombinedState -Raw $combinedRaw -WorkTarget $installDir -ToolingTarget 'C:\目錄 有空白\skills\tooling' -InstallerHost 'codex'
Assert-True ($null -ne $combined -and $combined.rust_workspace_version -eq '1.2.3') 'combined parent state read'
$legacyRaw = '{"host":"codex","version":"1.2.3","work_version":"1.2.3","tooling_version":"1.2.3","skill_install_dir":"C:\\目錄 有空白\\skills","global_agent_dir":null,"status":"installed"}'
$legacy = Read-InstallerCombinedState -Raw $legacyRaw -WorkTarget 'C:\目錄 有空白\skills\work' -ToolingTarget 'C:\目錄 有空白\skills\tooling' -InstallerHost 'codex'
Assert-True ($null -eq $legacy) 'legacy combined state rejected'
Assert-True ($null -eq (Read-InstallerCombinedState -Raw $combinedRaw -WorkTarget $installDir -ToolingTarget 'C:\目錄 有空白\skills\tooling')) 'missing parent Host rejected'
Assert-True ($null -eq (Read-InstallerCombinedState -Raw $combinedRaw -WorkTarget $installDir -ToolingTarget 'C:\目錄 有空白\skills\tooling' -InstallerHost 'claude')) 'wrong parent host'
Assert-True ($null -eq (Read-InstallerCombinedState -Raw ($combinedRaw -replace '"host":"codex",', '') -WorkTarget $installDir -ToolingTarget 'C:\目錄 有空白\skills\tooling' -InstallerHost 'codex')) 'legacy parent state'
Assert-True ((Get-InstallerToolingStateVersion -Raw '{"version":"1.0.0","skill":"tooling","tooling_dir":"C:\\tools\\tooling"}' -Target 'C:\tools\tooling') -eq '1.0.0') 'Tooling transaction state read'
$state = New-InstallerState -TemplateRaw $template -Version '1.2.3' -Skills 'java,spring-boot' -GlobalAgents $true -WorkDir $installDir -GlobalAgentsDir $globalDir -InstallerHost 'codex'
Assert-True (($state.skills -join ',') -ceq 'general,programming-language,java,spring-boot') 'parent Skill expansion'
Assert-True (@(Get-InstallerExpandedSkills -Selection 'all').Count -eq 14) 'all Skills'
$raw = ConvertTo-Json -InputObject $state -Depth 4 -Compress
$read = Read-InstallerState -Raw $raw -InstallDir $installDir -InstallerHost 'codex'
Assert-True ($null -ne $read -and $read.version -eq '1.2.3') 'valid state read'
Assert-True ($null -eq (Read-InstallerState -Raw $raw -InstallDir $installDir)) 'missing child Host rejected'
Assert-True ($null -eq (Read-InstallerState -Raw $raw -InstallDir $installDir -InstallerHost 'claude')) 'wrong child host'
Assert-True ($null -eq (Read-InstallerState -Raw ($raw -replace '"host":"codex",', '') -InstallDir $installDir -InstallerHost 'codex')) 'legacy child state'
Assert-True ($null -eq (Read-InstallerState -Raw ($raw -replace '"host":"codex"', '"host":"unknown"') -InstallDir $installDir -InstallerHost 'codex')) 'unknown child host'
Assert-True ($null -eq (Read-InstallerState -Raw $raw -InstallDir 'C:\other' -InstallerHost 'codex')) 'wrong target'
Assert-True ($null -eq (Read-InstallerState -Raw '' -InstallDir $installDir -InstallerHost 'codex')) 'empty state'
Assert-True ($null -eq (Read-InstallerState -Raw '{' -InstallDir $installDir -InstallerHost 'codex')) 'malformed state'
foreach ($change in @(
    @{ version = 'invalid' }, @{ skills = @('general', 'spring-boot') },
    @{ skills = @('general', 'general') }, @{ skills = @('unknown') },
    @{ work_dir = 'relative' }, @{ global_agents_dir = 'relative' },
    @{ global_agents_dir = $false }
)) {
    $copy = @{} + $state
    foreach ($key in $change.Keys) { $copy[$key] = $change[$key] }
    $invalid = ConvertTo-Json -InputObject $copy -Depth 4 -Compress
    Assert-True ($null -eq (Read-InstallerState -Raw $invalid -InstallDir $installDir -InstallerHost 'codex')) "invalid state: $($change.Keys -join ',')"
}
$copy = @{} + $state
$copy.extra = $true
Assert-True ($null -eq (Read-InstallerState -Raw (ConvertTo-Json -InputObject $copy -Depth 4 -Compress) -InstallDir $installDir -InstallerHost 'codex')) 'unknown field'
$copy = @{} + $state
$copy.Remove('version')
Assert-True ($null -eq (Read-InstallerState -Raw (ConvertTo-Json -InputObject $copy -Depth 4 -Compress) -InstallDir $installDir -InstallerHost 'codex')) 'missing field'
$legacy = '{"schema_version":1,"version":"1.2.3","skills":["general"],"global_agents":true,"install_dir":"C:\\work"}'
Assert-True ($null -eq (Read-InstallerState -Raw $legacy -InstallDir $installDir -InstallerHost 'codex')) 'previous shape is not current state'
$disabled = New-InstallerState -TemplateRaw $template -Version '1.2.3' -Skills 'general' -GlobalAgents $false -WorkDir $installDir -GlobalAgentsDir $globalDir -InstallerHost 'codex'
Assert-True ($null -eq $disabled.global_agents_dir) 'disabled Global AGENTS directory must be null'
$disabledRaw = ConvertTo-Json -InputObject $disabled -Depth 4 -Compress
Assert-True (-not (Resolve-InstallerSettings -Raw $disabledRaw -InstallDir $installDir -InstallerHost 'codex').GlobalAgents) 'null directory must disable Global AGENTS'
try {
    $null = New-InstallerState -TemplateRaw '{"version":null}' -Version '1.2.3' -Skills 'general' -GlobalAgents $true -WorkDir $installDir -GlobalAgentsDir $globalDir -InstallerHost 'codex'
    throw 'invalid template was accepted'
} catch {
    if ($_.Exception.Message -eq 'invalid template was accepted') { throw }
}
$default = Resolve-InstallerSettings -Raw '' -InstallDir $installDir -InstallerHost 'codex'
Assert-True (-not $default.HasValidState -and ($default.Skills -join ',') -eq 'general' -and
    $default.GlobalAgents -and $null -eq $default.GlobalAgentsDir) 'first install defaults'
$saved = Resolve-InstallerSettings -Raw $raw -InstallDir $installDir -InstallerHost 'codex'
Assert-True ($saved.HasValidState -and ($saved.Skills -join ',') -eq 'general,programming-language,java,spring-boot' -and
    $saved.GlobalAgents -and $saved.GlobalAgentsDir -eq $globalDir) 'saved settings'
$overridden = Resolve-InstallerSettings -Raw $raw -InstallDir $installDir -SkillsOverride 'web' -GlobalAgentsOverride $false -InstallerHost 'codex'
Assert-True (($overridden.Skills -join ',') -eq 'general,web' -and -not $overridden.GlobalAgents) 'CLI overrides'
Write-Output 'Windows install-state cases passed.'
