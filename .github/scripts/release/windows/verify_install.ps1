$ErrorActionPreference = 'Stop'
. $PSScriptRoot/firewall_isolation.ps1

function Test-ReleaseE2ERunnerConnection {
    param([Parameter(Mandatory)][string] $HostName)
    $addresses = @([Net.Dns]::GetHostAddresses($HostName) |
        Where-Object { $_.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork })
    if ($addresses.Count -eq 0) { throw "No IPv4 address for Actions endpoint: $HostName" }
    $address = $addresses[0].ToString()
    $client = [Net.Sockets.TcpClient]::new()
    try {
        if (-not $client.ConnectAsync($address, 443).Wait(7000)) {
            throw "GitHub Actions endpoint did not accept a connection: $HostName"
        }
    } finally {
        $client.Dispose()
    }
    return $address
}

$version = (Get-Content VERSION -Raw).Trim()
$rule = "agent-work-kit-release-e2e-$env:GITHUB_RUN_ID-$env:GITHUB_JOB"
$elapsed = [Diagnostics.Stopwatch]::StartNew()
$artifactHost = 'results-receiver.actions.githubusercontent.com'
$python = (Get-Command python.exe -ErrorAction Stop).Source
$programs = @(
    $python
    $env:ComSpec
    (Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe')
    (Get-Command cargo.exe -ErrorAction Stop).Source
    (Join-Path (Resolve-Path rust/target/release) 'work.exe')
    (Join-Path (Resolve-Path rust/target/release) 'skill.exe')
)
foreach ($program in $programs) {
    if (-not (Test-Path -LiteralPath $program -PathType Leaf)) {
        throw "E2E executable is missing: $program"
    }
}

# The supervisor is pwsh.exe and remains online. The E2E Python, batch,
# Windows PowerShell, Cargo and CLI executables are restricted by path.
$address = Test-ReleaseE2ERunnerConnection -HostName $artifactHost
Write-Host "GitHub Actions endpoint reachable before scoped E2E isolation: $address"
$env:AGENT_WORK_KIT_E2E_RULE_PREFIX = $rule
try {
    Invoke-ReleaseE2EIsolation -Rule $rule -Programs $programs -Verify {
        $probeAddress = Test-ReleaseE2ERunnerConnection -HostName $artifactHost
        Write-Host "Runner supervisor remains online during scoped E2E isolation: $probeAddress"

        # Probe from the restricted Python executable (not pwsh.exe).
        # A successful connection means the firewall rule did not take effect.
        $probeCode = 'import socket,sys;s=socket.socket();s.settimeout(3);sys.exit(0 if s.connect_ex((sys.argv[1],443)) == 0 else 1)'
        & $python -c $probeCode $probeAddress
        if ($LASTEXITCODE -ne 1) {
            throw "Python E2E network isolation failed: expected blocked connection, exit=$LASTEXITCODE"
        }
        Write-Host 'Restricted Python process cannot connect to the GitHub Actions endpoint.'

        foreach ($installerHost in @('codex', 'claude')) {
            $zip = "dist/agent-work-kit-v${version}-${installerHost}-windows-x64-msvc.zip"
            Write-Host "Scoped network isolation active; verifying Windows installation: $zip"
            & $python -B .github/scripts/release/release_install_e2e.py --zip $zip --checksums dist/SHA256SUMS.txt --platform windows-x64-msvc --source-install --installed-host-evidence dist/installed-host-simulation
            if ($LASTEXITCODE -ne 0) { throw "Windows release installation E2E failed for $installerHost with exit code $LASTEXITCODE" }
            Write-Host "Windows release installation E2E passed for $installerHost after $([math]::Round($elapsed.Elapsed.TotalSeconds, 1))s."
        }
        Write-Host 'Scoped network isolation active; verifying Windows dual Host coexistence.'
        & $python -B .github/scripts/release/release_install_e2e.py `
            --zip "dist/agent-work-kit-v${version}-codex-windows-x64-msvc.zip" `
            --peer-zip "dist/agent-work-kit-v${version}-claude-windows-x64-msvc.zip" `
            --checksums dist/SHA256SUMS.txt --platform windows-x64-msvc
        if ($LASTEXITCODE -ne 0) { throw "Windows dual Host coexistence E2E failed with exit code $LASTEXITCODE" }
    }
} finally {
    Remove-Item Env:AGENT_WORK_KIT_E2E_RULE_PREFIX -ErrorAction SilentlyContinue
    Write-Host "Windows release E2E elapsed: $([math]::Round($elapsed.Elapsed.TotalSeconds, 1))s."
}
$address = Test-ReleaseE2ERunnerConnection -HostName $artifactHost
Write-Host "GitHub Actions endpoint reachable after scoped E2E isolation: $address"
