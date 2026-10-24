$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/state/state-model.ps1')

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw "FAIL: $Message" }
}

$installDir = 'C:\目錄 有空白\skills\work'
$globalDir = 'C:\全域 有空白'
$template = [IO.File]::ReadAllText((Join-Path $repoRoot 'os-scripts/common/installer/install-state.json'))
$state = New-InstallerState -TemplateRaw $template -Version '1.2.3' -Skills 'java,spring-boot' -GlobalAgents $true -WorkDir $installDir -GlobalAgentsDir $globalDir
Assert-True (($state.skills -join ',') -ceq 'general,programming-language,java,spring-boot') 'parent Skill expansion'
Assert-True (@(Get-InstallerExpandedSkills -Selection 'all').Count -eq 14) 'all Skills'
$raw = ConvertTo-Json -InputObject $state -Depth 4 -Compress
$read = Read-InstallerState -Raw $raw -InstallDir $installDir
Assert-True ($null -ne $read -and $read.version -eq '1.2.3') 'valid state read'
Assert-True ($null -eq (Read-InstallerState -Raw $raw -InstallDir 'C:\other')) 'wrong target'
Assert-True ($null -eq (Read-InstallerState -Raw '' -InstallDir $installDir)) 'empty state'
Assert-True ($null -eq (Read-InstallerState -Raw '{' -InstallDir $installDir)) 'malformed state'
foreach ($change in @(
    @{ version = 'invalid' }, @{ skills = @('general', 'spring-boot') },
    @{ skills = @('general', 'general') }, @{ skills = @('unknown') },
    @{ work_dir = 'relative' }, @{ global_agents_dir = 'relative' },
    @{ global_agents_dir = $false }
)) {
    $copy = @{} + $state
    foreach ($key in $change.Keys) { $copy[$key] = $change[$key] }
    $invalid = ConvertTo-Json -InputObject $copy -Depth 4 -Compress
    Assert-True ($null -eq (Read-InstallerState -Raw $invalid -InstallDir $installDir)) "invalid state: $($change.Keys -join ',')"
}
$copy = @{} + $state
$copy.extra = $true
Assert-True ($null -eq (Read-InstallerState -Raw (ConvertTo-Json -InputObject $copy -Depth 4 -Compress) -InstallDir $installDir)) 'unknown field'
$copy = @{} + $state
$copy.Remove('version')
Assert-True ($null -eq (Read-InstallerState -Raw (ConvertTo-Json -InputObject $copy -Depth 4 -Compress) -InstallDir $installDir)) 'missing field'
$legacy = '{"schema_version":1,"version":"1.2.3","skills":["general"],"global_agents":true,"install_dir":"C:\\work"}'
Assert-True ($null -eq (Read-InstallerState -Raw $legacy -InstallDir $installDir)) 'previous shape is not current state'
$disabled = New-InstallerState -TemplateRaw $template -Version '1.2.3' -Skills 'general' -GlobalAgents $false -WorkDir $installDir -GlobalAgentsDir $globalDir
Assert-True ($null -eq $disabled.global_agents_dir) 'disabled Global AGENTS directory must be null'
$disabledRaw = ConvertTo-Json -InputObject $disabled -Depth 4 -Compress
Assert-True (-not (Resolve-InstallerSettings -Raw $disabledRaw -InstallDir $installDir).GlobalAgents) 'null directory must disable Global AGENTS'
try {
    $null = New-InstallerState -TemplateRaw '{"version":null}' -Version '1.2.3' -Skills 'general' -GlobalAgents $true -WorkDir $installDir -GlobalAgentsDir $globalDir
    throw 'invalid template was accepted'
} catch {
    if ($_.Exception.Message -eq 'invalid template was accepted') { throw }
}
$default = Resolve-InstallerSettings -Raw '' -InstallDir $installDir
Assert-True (-not $default.HasValidState -and ($default.Skills -join ',') -eq 'general' -and
    $default.GlobalAgents -and $null -eq $default.GlobalAgentsDir) 'first install defaults'
$saved = Resolve-InstallerSettings -Raw $raw -InstallDir $installDir
Assert-True ($saved.HasValidState -and ($saved.Skills -join ',') -eq 'general,programming-language,java,spring-boot' -and
    $saved.GlobalAgents -and $saved.GlobalAgentsDir -eq $globalDir) 'saved settings'
$overridden = Resolve-InstallerSettings -Raw $raw -InstallDir $installDir -SkillsOverride 'web' -GlobalAgentsOverride $false
Assert-True (($overridden.Skills -join ',') -eq 'general,web' -and -not $overridden.GlobalAgents) 'CLI overrides'
Write-Output 'Windows install-state cases passed.'
