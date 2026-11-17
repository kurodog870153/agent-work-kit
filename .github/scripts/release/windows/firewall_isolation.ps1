# E2E outbound restrictions apply only to named executables.
# Never create a machine-wide firewall rule: it can disconnect the Actions runner.
function Add-ReleaseE2EProgramRules {
    param(
        [Parameter(Mandatory)][string] $Rule,
        [Parameter(Mandatory)][string[]] $Programs
    )

    if ($Rule -notmatch '^[a-zA-Z0-9._-]+$') { throw 'Invalid E2E firewall rule prefix' }
    if ($Programs.Count -eq 0) { throw 'E2E firewall program list is empty' }

    foreach ($candidate in ($Programs | Sort-Object -Unique)) {
        if (-not [IO.Path]::IsPathRooted($candidate)) {
            throw "E2E firewall program must have an absolute path: $candidate"
        }
        $program = [IO.Path]::GetFullPath($candidate)
        $nameOnly = [IO.Path]::GetFileName($program)
        if (-not $nameOnly.EndsWith('.exe', [StringComparison]::OrdinalIgnoreCase)) {
            throw "E2E firewall program is not an executable: $program"
        }
        if ($nameOnly -in @('pwsh.exe', 'Runner.Worker.exe', 'Runner.Listener.exe')) {
            throw "Refusing to block the E2E supervisor or GitHub runner: $program"
        }

        $sha = [Security.Cryptography.SHA256]::Create()
        try {
            $hash = [BitConverter]::ToString(
                $sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($program.ToLowerInvariant()))
            ).Replace('-', '').Substring(0, 16).ToLowerInvariant()
        } finally {
            $sha.Dispose()
        }
        $name = "$Rule-$hash"
        $existing = Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue
        if (-not $existing) {
            New-NetFirewallRule -Name $name -DisplayName $name -Program $program `
                -Direction Outbound -Action Block -Profile Any -Enabled True -ErrorAction Stop | Out-Null
        }
        $active = Get-NetFirewallRule -Name $name -ErrorAction Stop
        $filters = @(Get-NetFirewallApplicationFilter -AssociatedNetFirewallRule $active -ErrorAction Stop)
        if ($active.Enabled -ne 'True' -or $active.Action -ne 'Block' -or
            $filters.Count -ne 1 -or
            -not [string]::Equals($filters[0].Program, $program, [StringComparison]::OrdinalIgnoreCase)) {
            throw "E2E firewall program rule is missing or mismatched: $name ($program)"
        }
    }
}

function Remove-ReleaseE2EProgramRules {
    param([Parameter(Mandatory)][string] $Rule)
    $problems = [System.Collections.Generic.List[string]]::new()
    foreach ($entry in @(Get-NetFirewallRule -Name "$Rule-*" -ErrorAction SilentlyContinue)) {
        try {
            Remove-NetFirewallRule -Name $entry.Name -ErrorAction Stop
        } catch {
            $problems.Add("remove $($entry.Name): $_")
        }
    }
    $remaining = @(Get-NetFirewallRule -Name "$Rule-*" -ErrorAction SilentlyContinue)
    if ($remaining.Count -ne 0) {
        $problems.Add("E2E firewall rules remain: $(($remaining.Name) -join ', ')")
    }
    if ($problems.Count -ne 0) { throw ($problems -join '; ') }
}

function Invoke-ReleaseE2EIsolation {
    param(
        [Parameter(Mandatory)][string] $Rule,
        [Parameter(Mandatory)][string[]] $Programs,
        [Parameter(Mandatory)][scriptblock] $Verify
    )

    $originalError = $null
    $cleanupError = $null
    try {
        Add-ReleaseE2EProgramRules -Rule $Rule -Programs $Programs
        & $Verify
    } catch {
        $originalError = $_
    } finally {
        Write-Host 'Removing scoped Windows release E2E network isolation rules.'
        try {
            Remove-ReleaseE2EProgramRules -Rule $Rule
            Write-Host 'Scoped Windows release E2E network isolation removed.'
        } catch {
            $cleanupError = $_
            Write-Host "Warning: Windows release E2E network isolation cleanup incomplete: $_"
        }
    }
    if ($originalError) { throw $originalError }
    if ($cleanupError) { throw $cleanupError }
}
