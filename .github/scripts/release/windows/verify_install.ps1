$ErrorActionPreference = 'Stop'

$version = (Get-Content VERSION -Raw).Trim()
$zip = "dist/work-v${version}-windows-x64-msvc.zip"
$rule = "work-release-e2e-$env:GITHUB_RUN_ID-$env:GITHUB_JOB"
Write-Host "Enabling outbound network isolation for Windows release installation E2E."
New-NetFirewallRule -Name $rule -DisplayName $rule -Direction Outbound -Action Block -Profile Any -Enabled True -ErrorAction Stop | Out-Null
try {
    $active = Get-NetFirewallRule -Name $rule -ErrorAction Stop
    if ($active.Enabled -ne 'True' -or $active.Action -ne 'Block') { throw 'Network isolation rule is inactive' }
    Write-Host "Network isolation active; verifying Windows release installation: $zip"
    python -B .github/scripts/release/release_install_e2e.py --zip $zip --checksums dist/SHA256SUMS.txt --platform windows-x64-msvc --source-install
    if ($LASTEXITCODE -ne 0) { throw "Windows release installation E2E failed with exit code $LASTEXITCODE" }
} finally {
    Write-Host 'Removing Windows release E2E network isolation rule.'
    Remove-NetFirewallRule -Name $rule -ErrorAction Stop
    for ($attempt = 1; $attempt -le 10; $attempt++) {
        if (-not (Get-NetFirewallRule -PolicyStore ActiveStore -Name $rule -ErrorAction SilentlyContinue)) { break }
        if ($attempt -eq 10) { throw 'Network isolation rule is still active' }
        Start-Sleep -Seconds 1
    }
    Clear-DnsClientCache
    Write-Host 'Windows release E2E network isolation removed; DNS cache cleared.'
}

$artifactHost = 'results-receiver.actions.githubusercontent.com'
for ($attempt = 1; $attempt -le 12; $attempt++) {
    try {
        Clear-DnsClientCache
        $addresses = [System.Net.Dns]::GetHostAddresses($artifactHost)
        if ($addresses.Count -eq 0) { throw 'Artifact host has no DNS address' }
        $client = [System.Net.Sockets.TcpClient]::new()
        try {
            if (-not $client.ConnectAsync($artifactHost, 443).Wait(5000)) { throw 'Artifact host is unreachable' }
        } finally {
            $client.Dispose()
        }
        Write-Host "Artifact network reachable after Windows isolation on attempt $attempt."
        break
    } catch {
        if ($attempt -eq 12) { throw "Artifact network did not recover: $_" }
        Start-Sleep -Seconds 5
    }
}
