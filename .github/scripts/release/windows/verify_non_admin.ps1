$ErrorActionPreference = 'Stop'
# CI-only environment preparation on the disposable runner.
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
$python = (Get-Command python.exe -ErrorAction Stop).Source
$runnerIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not ([Security.Principal.WindowsPrincipal]::new($runnerIdentity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))) {
    throw 'Standard User setup requires the elevated CI supervisor'
}
if ($env:GITHUB_SHA -notmatch '^[0-9a-f]{40}$') { throw 'Missing GitHub commit identity' }
$key = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
$name = 'AllowDevelopmentWithoutDevLicense'
$hadKey = Test-Path -LiteralPath $key
$previous = if ($hadKey) { (Get-ItemProperty -LiteralPath $key).PSObject.Properties[$name] } else { $null }

function Remove-TestProfile {
    param([string] $Sid)
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $profile = Get-CimInstance -ClassName Win32_UserProfile -Filter "SID='$Sid'"
        if ($null -eq $profile) { return }
        if (-not $profile.Loaded) {
            $profile | Remove-CimInstance -ErrorAction Stop
            return
        }
        Start-Sleep -Seconds 1
    }
    throw 'Temporary Standard User profile remained loaded after process cleanup'
}

$nonce = [guid]::NewGuid().ToString('N')
$username = 'awk-ci-' + $nonce.Substring(0, 12)
$stage = Join-Path (Split-Path -Parent $repo) ('awk-' + $nonce.Substring(0, 12))
$account = $null
$stageCreated = $false
$secure = $null
try {
    if (-not $hadKey) { New-Item -Path $key -Force | Out-Null }
    New-ItemProperty -LiteralPath $key -Name $name -PropertyType DWord -Value 0 -Force | Out-Null
    $secure = ConvertTo-SecureString ('Aa7!' + [guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')) -AsPlainText -Force
    $account = New-LocalUser -Name $username -Password $secure -Description 'Temporary Agent Work Kit CI acceptance' -UserMayNotChangePassword
    $sid = $account.SID.Value
    $users = [Security.Principal.SecurityIdentifier]::new('S-1-5-32-545')
    $admins = [Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
    if ($sid -notin @((Get-LocalGroupMember -SID $users).SID.Value)) {
        Add-LocalGroupMember -SID $users -Member $account
    }
    if ($sid -in @((Get-LocalGroupMember -SID $admins).SID.Value)) {
        throw 'Temporary CI account must not be an administrator'
    }
    New-Item -ItemType Directory -Path $stage -ErrorAction Stop | Out-Null
    $stageCreated = $true
    # No ACL changes to the original checkout or administrator profile.
    & icacls.exe $stage /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' "*$($runnerIdentity.User.Value):(OI)(CI)F" ('*' + $sid + ':(OI)(CI)M')
    if ($LASTEXITCODE -ne 0) { throw 'Failed to prepare Standard User staging ACL' }
    & $python -B (Join-Path $repo '.github/scripts/release/permission_smoke.py') --stage $repo $stage windows-x64-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Failed to stage representative package' }
    Write-Host "Prepared temporary Windows Standard User: SID=$sid; staging=$stage"
    $credential = [pscredential]::new(".\$username", $secure)
    $script = Join-Path $stage 'release/permission_smoke.py'
    $arguments = '-u -B "{0}" --windows "{1}" {2} {3}' -f $script, $stage, $sid, $env:GITHUB_SHA
    $log = Join-Path $stage 'acceptance.log'
    # With alternate credentials, UseNewEnvironment lets Windows build the
    # new user's environment instead of copying the elevated runner's variables.
    $process = Start-Process -FilePath $python -ArgumentList $arguments -Credential $credential -LoadUserProfile -UseNewEnvironment -WorkingDirectory $stage -PassThru
    try {
        if (-not $process.WaitForExit(180000)) { throw 'Standard User permission smoke timed out' }
        if ($process.ExitCode -ne 0) { throw "Standard User permission smoke failed: $($process.ExitCode)" }
        if (-not (Test-Path -LiteralPath $log -PathType Leaf)) { throw 'Standard User permission smoke log missing' }
    } finally {
        try {
            if (-not $process.HasExited) {
                $process.Kill($true)
                if (-not $process.WaitForExit(10000)) { throw 'Standard User process cleanup timed out' }
            }
        } finally {
            $process.Dispose()
            if (Test-Path -LiteralPath $log -PathType Leaf) { Get-Content -LiteralPath $log -Encoding UTF8 | Write-Host }
        }
    }
} finally {
    if ($null -ne $secure) { $secure.Dispose() }
    try {
        if ($null -ne $account) {
            try { Remove-TestProfile -Sid $account.SID.Value }
            finally { Remove-LocalUser -SID $account.SID -ErrorAction Stop }
        }
    } finally {
        try {
            if ($stageCreated) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction Stop }
        } finally {
            if ($null -ne $previous) {
                New-ItemProperty -LiteralPath $key -Name $name -PropertyType DWord -Value $previous.Value -Force | Out-Null
            } else {
                Remove-ItemProperty -LiteralPath $key -Name $name -ErrorAction SilentlyContinue
            }
            if (-not $hadKey -and (Test-Path -LiteralPath $key) -and
                (Get-Item -LiteralPath $key).ValueCount -eq 0 -and (Get-Item -LiteralPath $key).SubKeyCount -eq 0) {
                Remove-Item -LiteralPath $key
            }
        }
    }
}
Write-Host 'Standard User acceptance environment cleaned up.'
