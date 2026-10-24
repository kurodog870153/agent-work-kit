$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/lock/mutex.ps1')
$result = Invoke-InstallerLocked -Transaction { param($value) "ok:$value" } -Arguments @('normal')
if ($result -ne 'ok:normal') { throw 'normal transaction failed' }
$scratch = Join-Path $repoRoot ('.issue95-lock.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolvedScratch = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolvedScratch.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolvedScratch) -match '^\.issue95-lock\.[0-9a-f]{32}$')) { throw 'unsafe test cleanup path' }
$marker = Join-Path $scratch 'ready'
$job = $null
try {
    $job = Start-Job -ArgumentList $marker -ScriptBlock {
        param($path)
        $mutex = [Threading.Mutex]::new($false, 'Global\CodexWorkflowKitWorkInstall')
        $null = $mutex.WaitOne(0)
        Set-Content -LiteralPath $path -Value 'ready'
        Start-Sleep -Seconds 30
        $mutex.ReleaseMutex()
        $mutex.Dispose()
    }
    for ($attempt = 0; $attempt -lt 100 -and -not (Test-Path -LiteralPath $marker); $attempt++) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path -LiteralPath $marker)) { throw 'lock holder did not start' }
    try {
        $null = Invoke-InstallerLocked -Transaction { 'unexpected' }
        throw 'concurrent installation acquired lock'
    } catch {
        if ($_.Exception.Message -notmatch 'another Work installation') { throw }
    }
    Stop-Job $job
    $null = Wait-Job $job
    $result = Invoke-InstallerLocked -Transaction { 'after-abandon' }
    if ($result -ne 'after-abandon') { throw 'abandoned mutex was not recovered' }
    Write-Output 'Windows native lock cases passed.'
} finally {
    if ($null -ne $job) { Stop-Job $job -ErrorAction SilentlyContinue; Remove-Job $job -Force -ErrorAction SilentlyContinue }
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolvedScratch) { Remove-Item -LiteralPath $resolvedScratch -Recurse -Force }
}
