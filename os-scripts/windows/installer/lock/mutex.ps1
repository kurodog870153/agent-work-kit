Set-StrictMode -Version Latest

# One lock per OS account across Hosts, targets and login sessions.
function Get-InstallerUserLockName {
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    return "Global\AgentWorkKitInstall.User.$sid"
}

function Invoke-InstallerLocked {
    param([Parameter(Mandatory = $true)][scriptblock] $Transaction)
    $mutex = [System.Threading.Mutex]::new($false, (Get-InstallerUserLockName))
    $owned = $false
    try {
        try { $owned = $mutex.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $owned = $true }
        if (-not $owned) { throw 'another installation is running' }
        & $Transaction
    } finally {
        try { if ($owned) { $mutex.ReleaseMutex() } }
        finally { $mutex.Dispose() }
    }
}
