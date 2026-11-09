param([string] $Binary, [string] $ToolingBinary)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
if ([string]::IsNullOrEmpty($Binary)) { $Binary = Join-Path $repoRoot 'rust/target/release/work.exe' }
if ([string]::IsNullOrEmpty($ToolingBinary)) { $ToolingBinary = Join-Path $repoRoot 'rust/target/release/tooling.exe' }
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw 'native Work binary is required' }
if (-not (Test-Path -LiteralPath $ToolingBinary -PathType Leaf)) { throw 'native Tooling binary is required' }
$scratch = Join-Path $repoRoot ('.pkgtest.' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$resolved = (Resolve-Path -LiteralPath $scratch).Path
if (-not $resolved.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not ([IO.Path]::GetFileName($resolved) -match '^\.pkgtest\.[0-9a-f]{32}$')) { throw 'unsafe cleanup path' }
$originalUserProfile = $env:USERPROFILE
$originalHome = $env:HOME
$testHome = Join-Path $scratch 'home'
New-Item -ItemType Directory -Path $testHome | Out-Null
$env:USERPROFILE = $testHome
$env:HOME = $testHome
try {
    $output = Join-Path $scratch 'dist'
    $commit = (git rev-parse HEAD).Trim()
    foreach ($installerHost in @('codex', 'claude')) {
        & python -B (Join-Path $repoRoot '.github/scripts/release/release_package.py') package --repository $repoRoot --output $output --host $installerHost --platform windows-x64-msvc --binary $Binary --tooling-binary $ToolingBinary --commit $commit --runner-label windows-latest --runner-os Windows --runner-arch X64 --runner-image test-windows --runner-image-version 1 --os-name Windows --os-version 10 --os-build 1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Windows $installerHost package build failed" }
        $version = (Get-Content (Join-Path $repoRoot 'VERSION') -Raw).Trim()
        $archive = Join-Path $output "agent-work-kit-v${version}-${installerHost}-windows-x64-msvc.zip"
        if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) { throw 'expected Windows Host ZIP' }
        $package = Join-Path $scratch "p-$installerHost"
        Expand-Archive -LiteralPath $archive -DestinationPath $package
        $entry = Join-Path $package 'install-agent-work-kit-from-package.bat'
        if (-not (Test-Path -LiteralPath $entry)) { throw 'new Package entry is missing' }
        $skillsParent = Join-Path $scratch "i-$installerHost"
        $target = Join-Path $skillsParent 'work'
        & $entry --host $installerHost --non-interactive --global-instructions no --skills-dir $skillsParent | Out-Null
        $statePath = Join-Path $skillsParent 'install-state.json'
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $statePath) -or
            (Test-Path -LiteralPath (Join-Path $target 'install-state.json')) -or
            (Test-Path -LiteralPath (Join-Path $skillsParent 'tooling/install-state.json')) -or
            -not (Test-Path -LiteralPath (Join-Path $skillsParent 'tooling/scripts/tooling.exe'))) {
            throw "Windows $installerHost offline Package installation failed"
        }
        $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
        $releaseInfo = Get-Content -LiteralPath (Join-Path $package 'release-info.json') -Raw | ConvertFrom-Json
        if ($state.host -cne $installerHost -or $releaseInfo.host -cne $installerHost -or
            $state.status -ne 'installed' -or $state.version -ne $releaseInfo.version -or
            $state.rust_workspace_version -ne $releaseInfo.rust_workspace_version -or
            $state.skill_install_dir -ne $skillsParent) { throw 'Windows Package state is invalid' }
        $digest = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        $checksums = Join-Path $output 'SHA256SUMS.txt'
        [IO.File]::WriteAllText($checksums, "$digest  $([IO.Path]::GetFileName($archive))`n")
        & python -B (Join-Path $repoRoot '.github/scripts/release/release_downloader_e2e.py') `
            --zip $archive --checksums $checksums --platform windows-x64-msvc
        if ($LASTEXITCODE -ne 0) { throw "Windows $installerHost Downloader E2E failed" }
    }
    Write-Output 'Windows Package entry fixture passed.'
} finally {
    $env:USERPROFILE = $originalUserProfile
    $env:HOME = $originalHome
    if ((Resolve-Path -LiteralPath $scratch).Path -eq $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
