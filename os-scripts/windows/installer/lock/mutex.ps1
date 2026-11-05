Set-StrictMode -Version Latest

function Invoke-InstallerLocked {
    param(
        [Parameter(Mandatory = $true)][scriptblock] $Transaction,
        [object[]] $Arguments = @(),
        [ValidateSet('codex', 'claude')][string] $InstallerHost
    )
    if ($InstallerHost -cnotin @('codex', 'claude')) { throw 'invalid installer host' }
    $mutexName = if ($InstallerHost -eq 'codex') { 'Global\AgentWorkKitInstallCodex' } else { 'Global\AgentWorkKitInstallClaude' }
    $mutex = [System.Threading.Mutex]::new($false, $mutexName)
    $owned = $false
    try {
        try {
            $owned = $mutex.WaitOne(0)
        } catch [System.Threading.AbandonedMutexException] {
            $owned = $true
        }
        if (-not $owned) { throw 'another Work installation is running' }
        return & $Transaction @Arguments
    } finally {
        if ($owned) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}
