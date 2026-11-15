$ErrorActionPreference = 'Stop'

# Copy the NSIS setup of the current version into ./installer (gitignored).
$root = Split-Path -Parent $PSScriptRoot
$version = (Get-Content -Raw (Join-Path $root 'src-tauri/tauri.conf.json') | ConvertFrom-Json).version
$setup = Join-Path $root "src-tauri/target/release/bundle/nsis/FujiView_${version}_x64-setup.exe"
if (-not (Test-Path -LiteralPath $setup)) { throw "Installer non trovato: $setup. Esegui prima 'make build'." }

$out = Join-Path $root 'installer'
New-Item -ItemType Directory -Force -Path $out | Out-Null
Get-ChildItem -LiteralPath $out -Filter 'FujiView_*-setup.exe' | Remove-Item -Force
Copy-Item -LiteralPath $setup -Destination $out
Write-Host "Installer: $(Join-Path $out (Split-Path -Leaf $setup))"
