Set-StrictMode -Version Latest

function Invoke-InstallerLocked {
    param(
        [Parameter(Mandatory = $true)][scriptblock] $Transaction,
        [object[]] $Arguments = @()
    )
    $mutex = [System.Threading.Mutex]::new($false, 'Global\CodexWorkflowKitWorkInstall')
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
