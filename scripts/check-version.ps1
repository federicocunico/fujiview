param(
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

$versionFile = (Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'VERSION')).Trim()
$packageVersion = (Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'package.json') | ConvertFrom-Json).version
$packageLockText = Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'package-lock.json')
$packageLockVersionMatch = [regex]::Match($packageLockText, '(?s)^\s*\{.*?"version"\s*:\s*"([^"]+)"')
$packageLockRootVersionMatch = [regex]::Match($packageLockText, '(?s)"packages"\s*:\s*\{\s*""\s*:\s*\{.*?"version"\s*:\s*"([^"]+)"')

if (-not $packageLockVersionMatch.Success -or -not $packageLockRootVersionMatch.Success) {
    throw 'Unable to read the root versions from package-lock.json.'
}

$packageLockVersion = $packageLockVersionMatch.Groups[1].Value
$packageLockRootVersion = $packageLockRootVersionMatch.Groups[1].Value
$tauriVersion = (Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'src-tauri/tauri.conf.json') | ConvertFrom-Json).version
$cargoText = Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'src-tauri/Cargo.toml')
$cargoMatch = [regex]::Match($cargoText, '(?m)^version\s*=\s*"([^"]+)"')

if (-not $cargoMatch.Success) {
    throw 'Unable to read the version from src-tauri/Cargo.toml.'
}

$cargoVersion = $cargoMatch.Groups[1].Value
$versions = @($versionFile, $packageVersion, $packageLockVersion, $packageLockRootVersion, $tauriVersion, $cargoVersion) | Select-Object -Unique
if ($versions.Count -ne 1) {
    throw "Versions are not aligned: VERSION=$versionFile, package.json=$packageVersion, package-lock.json=$packageLockVersion/$packageLockRootVersion, tauri.conf.json=$tauriVersion, Cargo.toml=$cargoVersion"
}

if ($packageVersion -notmatch '^\d+\.\d+\.\d+([-.][0-9A-Za-z.-]+)?$') {
    throw "Invalid version: $packageVersion"
}

if ($Tag) {
    $expectedTag = "v$packageVersion"
    if ($Tag -ne $expectedTag) {
        throw "Tag $Tag does not match application version $expectedTag."
    }
}

Write-Output "FujiView version: $packageVersion"
