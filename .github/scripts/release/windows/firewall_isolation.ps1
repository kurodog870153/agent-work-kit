function Invoke-ReleaseE2EIsolation {
    param([string] $Rule, [scriptblock] $Verify)

    $cleanupProblems = [System.Collections.Generic.List[string]]::new()
    $originalError = $null
    New-NetFirewallRule -Name $Rule -DisplayName $Rule -Direction Outbound -Action Block -Profile Any -Enabled True -ErrorAction Stop | Out-Null
    try {
        $active = Get-NetFirewallRule -Name $Rule -ErrorAction Stop
        if ($active.Enabled -ne 'True' -or $active.Action -ne 'Block') { throw 'Network isolation rule is inactive' }
        & $Verify
    } catch {
        $originalError = $_
    } finally {
        Write-Host 'Removing Windows release E2E network isolation rule.'
        try {
            Remove-NetFirewallRule -Name $Rule -ErrorAction Stop
        } catch {
            $cleanupProblems.Add("remove isolation rule: $_")
        }
        for ($attempt = 1; $attempt -le 10; $attempt++) {
            if (-not (Get-NetFirewallRule -PolicyStore ActiveStore -Name $Rule -ErrorAction SilentlyContinue)) { break }
            if ($attempt -eq 10) { $cleanupProblems.Add('network isolation rule is still active') }
            Start-Sleep -Seconds 1
        }
        try {
            Clear-DnsClientCache
        } catch {
            $cleanupProblems.Add("clear DNS cache: $_")
        }
        if ($cleanupProblems.Count) {
            Write-Host "Warning: Windows release E2E network isolation cleanup incomplete: $($cleanupProblems -join '; ')"
        } else {
            Write-Host 'Windows release E2E network isolation removed; DNS cache cleared.'
        }
    }
    if ($originalError) { throw $originalError }
    if ($cleanupProblems.Count) {
        throw "Windows release E2E network isolation cleanup failed: $($cleanupProblems -join '; ')"
    }
}
