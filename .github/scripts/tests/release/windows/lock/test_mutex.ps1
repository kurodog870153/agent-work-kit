param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('normal', 'hold', 'contend', 'recover', 'user-name')]
    [string] $Mode,
    [string] $Marker
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repoRoot 'os-scripts/windows/installer/lock/mutex.ps1')

switch ($Mode) {
    'user-name' { Get-InstallerUserLockName }
    'normal' {
        $result = Invoke-InstallerLocked -Transaction { 'ok' }
        if ($result -ne 'ok') { throw 'normal transaction failed' }
    }
    'hold' {
        if ([string]::IsNullOrWhiteSpace($Marker)) { throw 'holder marker is required' }
        Invoke-InstallerLocked -Transaction {
            Set-Content -LiteralPath $Marker -Value 'ready'
            Start-Sleep -Seconds 120
            throw 'lock holder was not terminated by test controller'
        }
    }
    'contend' {
        try {
            $null = Invoke-InstallerLocked -Transaction { throw 'concurrent installation acquired lock' }
            throw 'concurrent installation did not reject the lock'
        } catch {
            if ($_.Exception.Message -ne 'another installation is running') { throw }
        }
    }
    'recover' {
        $result = Invoke-InstallerLocked -Transaction { 'after-abandon' }
        if ($result -ne 'after-abandon') { throw 'abandoned mutex was not recovered' }
    }
}
