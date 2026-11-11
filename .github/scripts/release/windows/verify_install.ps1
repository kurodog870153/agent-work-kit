$ErrorActionPreference = 'Stop'
. $PSScriptRoot/firewall_isolation.ps1

$version = (Get-Content VERSION -Raw).Trim()
$rule = "agent-work-kit-release-e2e-$env:GITHUB_RUN_ID-$env:GITHUB_JOB"
$elapsed = [Diagnostics.Stopwatch]::StartNew()
Write-Host "Enabling outbound network isolation for Windows release installation E2E."
try {
    Invoke-ReleaseE2EIsolation -Rule $rule -Verify {
        foreach ($installerHost in @('codex', 'claude')) {
            $zip = "dist/agent-work-kit-v${version}-${installerHost}-windows-x64-msvc.zip"
            Write-Host "Network isolation active; verifying Windows release installation: $zip"
            python -B .github/scripts/release/release_install_e2e.py --zip $zip --checksums dist/SHA256SUMS.txt --platform windows-x64-msvc --source-install --cli-smoke-report dist/cli-smoke
            if ($LASTEXITCODE -ne 0) { throw "Windows release installation E2E failed for $installerHost with exit code $LASTEXITCODE" }
            Write-Host "Windows release installation E2E passed for $installerHost after $([math]::Round($elapsed.Elapsed.TotalSeconds, 1))s."
        }
        Write-Host 'Network isolation active; verifying Windows dual Host coexistence.'
        python -B .github/scripts/release/release_install_e2e.py `
            --zip "dist/agent-work-kit-v${version}-codex-windows-x64-msvc.zip" `
            --peer-zip "dist/agent-work-kit-v${version}-claude-windows-x64-msvc.zip" `
            --checksums dist/SHA256SUMS.txt --platform windows-x64-msvc
        if ($LASTEXITCODE -ne 0) { throw "Windows dual Host coexistence E2E failed with exit code $LASTEXITCODE" }
    }
} finally {
    Write-Host "Windows release installation E2E elapsed: $([math]::Round($elapsed.Elapsed.TotalSeconds, 1))s."
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
