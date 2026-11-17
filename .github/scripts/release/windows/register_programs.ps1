# Add scoped firewall rules for the dynamically extracted and installed CLI binaries.
# Invoked by the E2E driver while its supervisor retains the cleanup responsibility.
param(
    [Parameter(Mandatory)][string] $Rule,
    [Parameter(Mandatory)][string] $PathsFile
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/firewall_isolation.ps1"

$json = [IO.File]::ReadAllText($PathsFile)
$trimmed = $json.Trim()
if (-not $trimmed.StartsWith('[') -or -not $trimmed.EndsWith(']')) {
    throw 'Invalid CLI program list for E2E firewall isolation'
}

# Windows PowerShell may emit a JSON array as one nested Object[] pipeline
# value; enumerate it explicitly instead of wrapping ConvertFrom-Json in @().
$decoded = ConvertFrom-Json -InputObject $json -ErrorAction Stop
$paths = @()
foreach ($item in $decoded) {
    if ($item -isnot [string] -or [string]::IsNullOrWhiteSpace($item)) {
        throw 'Invalid CLI program list for E2E firewall isolation'
    }
    $paths += $item
}
if ($paths.Count -eq 0) { throw 'Invalid CLI program list for E2E firewall isolation' }
Add-ReleaseE2EProgramRules -Rule $Rule -Programs ([string[]] $paths)
