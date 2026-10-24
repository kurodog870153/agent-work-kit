param([string] $Binary)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
if ([string]::IsNullOrEmpty($Binary)) { $Binary = Join-Path $repoRoot 'rust/target/release/work.exe' }
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw 'native Work binary is required' }
$scratch = Join-Path $repoRoot ('.issue95-package.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolved = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolved.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolved) -match '^\.issue95-package\.[0-9a-f]{32}$')) { throw 'unsafe cleanup path' }
$originalUserProfile = $env:USERPROFILE
$originalHome = $env:HOME
$testHome = Join-Path $scratch 'home'
New-Item -ItemType Directory -Path $testHome | Out-Null
$env:USERPROFILE = $testHome
$env:HOME = $testHome
try {
    $output = Join-Path $scratch 'dist'
    $commit = (git rev-parse HEAD).Trim()
    & python -B (Join-Path $repoRoot '.github/scripts/release/release_package.py') package --repository $repoRoot --output $output --platform windows-x64-msvc --binary $Binary --commit $commit --runner-label windows-latest --runner-os Windows --runner-arch X64 --runner-image test-windows --runner-image-version 1 --os-name Windows --os-version 10 --os-build 1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Windows package build failed' }
    $archive = @(Get-ChildItem -LiteralPath $output -Filter '*.zip')
    if ($archive.Count -ne 1) { throw 'expected one Windows ZIP' }
    $package = Join-Path $scratch 'unpacked'
    Expand-Archive -LiteralPath $archive[0].FullName -DestinationPath $package
    $entry = Join-Path $package 'install-work-from-package.bat'
    if (-not (Test-Path -LiteralPath $entry)) { throw 'new Package entry is missing' }
    $target = Join-Path $scratch 'installed/work'
    & $entry --non-interactive --no-global-agents --skills general --work-install-dir $target | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $target 'install-state.json'))) {
        throw 'Windows offline Package installation failed'
    }
    Write-Output 'Windows Package entry fixture passed.'
} finally {
    $env:USERPROFILE = $originalUserProfile
    $env:HOME = $originalHome
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
