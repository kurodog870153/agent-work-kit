Set-StrictMode -Version Latest

function Get-InstallerFileSha256 {
    param([Parameter(Mandatory = $true)][string] $Path)
    $stream = [IO.File]::OpenRead($Path)
    try {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try {
            return [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace('-', '').ToLowerInvariant()
        } finally {
            $hasher.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

function Get-InstallerGlobalAgentsPlan {
    param(
        [Parameter(Mandatory = $true)][string] $Source,
        [Parameter(Mandatory = $true)][string] $Target,
        [bool] $Enabled,
        [bool] $NonInteractive,
        [bool] $Confirmed
    )
    if (-not $Enabled) { return [pscustomobject]@{ Action = 'skip'; SourceSha256 = ''; TargetSha256 = '' } }
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) { throw "Global AGENTS source does not exist: $Source" }
    $sourceItem = Get-Item -LiteralPath $Source -Force
    if (($sourceItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Global AGENTS source is a link' }
    $sourceHash = Get-InstallerFileSha256 -Path $Source
    if (-not (Test-Path -LiteralPath $Target)) {
        return [pscustomobject]@{ Action = 'install'; SourceSha256 = $sourceHash; TargetSha256 = '' }
    }
    $targetItem = Get-Item -LiteralPath $Target -Force
    if ($targetItem.PSIsContainer -or ($targetItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'Global AGENTS target is not a regular file'
    }
    $targetHash = Get-InstallerFileSha256 -Path $Target
    if ($sourceHash -ceq $targetHash) {
        return [pscustomobject]@{ Action = 'unchanged'; SourceSha256 = $sourceHash; TargetSha256 = $targetHash }
    }
    if ($NonInteractive) { throw 'Global AGENTS differs and requires interactive confirmation' }
    $action = if ($Confirmed) { 'replace' } else { 'confirm' }
    return [pscustomobject]@{ Action = $action; SourceSha256 = $sourceHash; TargetSha256 = $targetHash }
}
