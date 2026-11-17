Set-StrictMode -Version Latest

# Returns @{ Action = skip|install|unchanged|replace|confirm; SourceSha256; TargetSha256 }; throws on unsafe or unauthorized targets.
# A target equal to the previously installed copy is still ours and may be replaced.
function Get-InstallerGlobalPlan {
    param([string] $Source, [string] $Target, [bool] $Enabled, [bool] $NonInteractive, [string] $PreviousSha)
    if (-not $Enabled) { return @{ Action = 'skip'; SourceSha256 = ''; TargetSha256 = '' } }
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf) -or (Test-InstallerReparse (Get-Item -LiteralPath $Source -Force))) {
        throw 'Global Instructions source is missing or a link'
    }
    $sourceHash = Get-InstallerFileSha256 -Path $Source
    if (-not (Test-Path -LiteralPath $Target)) { return @{ Action = 'install'; SourceSha256 = $sourceHash; TargetSha256 = '' } }
    $item = Get-Item -LiteralPath $Target -Force
    if ($item.PSIsContainer -or (Test-InstallerReparse $item)) { throw 'Global Instructions target is not a regular file' }
    $targetHash = Get-InstallerFileSha256 -Path $Target
    if ($sourceHash -ceq $targetHash) { return @{ Action = 'unchanged'; SourceSha256 = $sourceHash; TargetSha256 = $targetHash } }
    if (-not [string]::IsNullOrEmpty($PreviousSha) -and $targetHash -ceq $PreviousSha) {
        return @{ Action = 'replace'; SourceSha256 = $sourceHash; TargetSha256 = $targetHash }
    }
    if ($NonInteractive) { throw 'Global Instructions differ and need interactive confirmation' }
    return @{ Action = 'confirm'; SourceSha256 = $sourceHash; TargetSha256 = $targetHash }
}
