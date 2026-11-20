Set-StrictMode -Version Latest

# One machine-wide installation lock for every Host and target.
function Invoke-InstallerLocked {
    param([Parameter(Mandatory = $true)][scriptblock] $Transaction)
    $mutex = [System.Threading.Mutex]::new($false, 'Global\AgentWorkKitInstall')
    $owned = $false
    try {
        try { $owned = $mutex.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $owned = $true }
        if (-not $owned) { throw 'another installation is running' }
        & $Transaction
    } finally {
        if ($owned) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}
