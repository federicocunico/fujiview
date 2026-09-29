[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$versionPath = Join-Path $repoRoot 'VERSION'

if (-not (Test-Path -LiteralPath $versionPath)) {
    throw 'VERSION file not found.'
}

$version = (Get-Content -Raw -LiteralPath $versionPath).Trim()
& (Join-Path $PSScriptRoot 'check-version.ps1')

$bundleDirectory = Join-Path $repoRoot 'src-tauri/target/release/bundle/nsis'
$installer = Join-Path $bundleDirectory "FujiView_${version}_x64-setup.exe"
if (-not (Test-Path -LiteralPath $installer)) {
    throw "Installer not found: $installer. Run make build first."
}

Write-Output "Installing FujiView $version..."
$process = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru -WindowStyle Hidden
if ($process.ExitCode -ne 0) {
    throw "FujiView installer failed with exit code $($process.ExitCode)."
}

Write-Output "FujiView $version installed successfully."
