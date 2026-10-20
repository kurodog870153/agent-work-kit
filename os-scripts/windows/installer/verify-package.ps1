param([Parameter(Mandatory = $true)][string]$PackageRoot)

$ErrorActionPreference = 'Stop'
Import-Module Microsoft.PowerShell.Utility -ErrorAction Stop

try {
    $root = (Get-Item -LiteralPath $PackageRoot -Force).FullName.TrimEnd('\', '/')
    if (-not (Test-Path -LiteralPath $root -PathType Container)) { throw 'Package root is not a directory.' }
    $manifestPath = Join-Path $root 'SHA256SUMS'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw 'SHA256SUMS is missing.' }
    $utf8 = New-Object System.Text.UTF8Encoding($false, $true)
    $bytes = [IO.File]::ReadAllBytes($manifestPath)
    if ($bytes.Length -eq 0 -or $bytes[$bytes.Length - 1] -ne 10) { throw 'SHA256SUMS must end with LF.' }
    if ($bytes.Length -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191) { throw 'SHA256SUMS must not have a BOM.' }
    if ($bytes -contains 13) { throw 'SHA256SUMS must use LF lines.' }
    $content = $utf8.GetString($bytes)
    $expected = New-Object 'System.Collections.Generic.Dictionary[string,string]' ([StringComparer]::OrdinalIgnoreCase)
    $lines = $content.TrimEnd("`n").Split("`n")
    foreach ($line in $lines) {
        if ($line -cnotmatch '^([0-9a-f]{64})  ([A-Za-z0-9._/-]+)$') { throw "Invalid SHA256SUMS line: $line" }
        $name = $Matches[2]
        if ($name -eq 'SHA256SUMS' -or $name.StartsWith('/') -or $name.EndsWith('/') -or $name.Contains('//') -or @($name.Split('/') | Where-Object { $_ -eq '.' -or $_ -eq '..' }).Count -gt 0) {
            throw "Unsafe SHA256SUMS path: $name"
        }
        if ($expected.ContainsKey($name)) { throw "Duplicate SHA256SUMS path: $name" }
        $expected.Add($name, $Matches[1])
    }
    if ($expected.Count -eq 0) { throw 'SHA256SUMS is empty.' }

    $found = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    $pending = New-Object 'System.Collections.Generic.Stack[string]'
    $pending.Push($root)
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        foreach ($item in (Get-ChildItem -LiteralPath $directory -Force)) {
            $relative = $item.FullName.Substring($root.Length + 1).Replace('\', '/')
            if ($relative -cnotmatch '^[A-Za-z0-9._/-]+$' -or $relative.Contains('//') -or @($relative.Split('/') | Where-Object { $_ -eq '.' -or $_ -eq '..' }).Count -gt 0) {
                throw "Unsafe package path: $relative"
            }
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse point in package: $relative" }
            if ($item.PSIsContainer) {
                $pending.Push($item.FullName)
                continue
            }
            if ($relative -eq 'SHA256SUMS') { continue }
            if (-not $expected.ContainsKey($relative)) { throw "Extra package file: $relative" }
            if (-not $found.Add($relative)) { throw "Duplicate package file: $relative" }
            $actual = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actual -cne $expected[$relative]) { throw "SHA-256 mismatch: $relative" }
        }
    }
    if ($found.Count -ne $expected.Count) { throw 'Package file list differs from SHA256SUMS.' }
    Write-Output 'Release package SHA-256 verification passed.'
} catch {
    [Console]::Error.WriteLine("Release integrity check failed: $($_.Exception.Message)")
    exit 1
}
