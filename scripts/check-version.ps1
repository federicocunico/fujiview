param(
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

$packageVersion = (Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'package.json') | ConvertFrom-Json).version
$tauriVersion = (Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'src-tauri/tauri.conf.json') | ConvertFrom-Json).version
$cargoText = Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'src-tauri/Cargo.toml')
$cargoMatch = [regex]::Match($cargoText, '(?m)^version\s*=\s*"([^"]+)"')

if (-not $cargoMatch.Success) {
    throw 'Impossibile leggere la versione da src-tauri/Cargo.toml.'
}

$cargoVersion = $cargoMatch.Groups[1].Value
$versions = @($packageVersion, $tauriVersion, $cargoVersion) | Select-Object -Unique
if ($versions.Count -ne 1) {
    throw "Versioni non allineate: package.json=$packageVersion, tauri.conf.json=$tauriVersion, Cargo.toml=$cargoVersion"
}

if ($packageVersion -notmatch '^\d+\.\d+\.\d+([-.][0-9A-Za-z.-]+)?$') {
    throw "Versione non valida: $packageVersion"
}

if ($Tag) {
    $expectedTag = "v$packageVersion"
    if ($Tag -ne $expectedTag) {
        throw "Il tag $Tag non corrisponde alla versione applicativa $expectedTag."
    }
}

Write-Output "FujiView version: $packageVersion"
