[CmdletBinding()]
param(
    [switch]$InstallMissing,
    [switch]$CheckOnly
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not $IsWindows -and $PSVersionTable.PSEdition -eq 'Core') {
    throw 'FujiView supporta attualmente Windows 11. Esegui il bootstrap su Windows.'
}
function Test-Command {
    param([Parameter(Mandatory = $true)][string]$Name)
    return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Test-MsvcBuildTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { return $false }
    $installation = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return $LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace(($installation -join ''))
}

function Test-WebView2 {
    $locations = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Microsoft/EdgeWebView/Application/*/msedgewebview2.exe'),
        (Join-Path $env:ProgramFiles 'Microsoft/EdgeWebView/Application/*/msedgewebview2.exe'),
        (Join-Path $env:LOCALAPPDATA 'Microsoft/EdgeWebView/Application/*/msedgewebview2.exe')
    )
    return $null -ne ($locations | Where-Object { Test-Path $_ } | Select-Object -First 1)
}

function Install-WingetPackage {
    param(
        [Parameter(Mandatory = $true)][string]$Id,
        [string]$Override
    )
    if (-not (Test-Command 'winget')) {
        throw "winget non è disponibile. Installa App Installer dal Microsoft Store, poi riesegui make setup. Pacchetto richiesto: $Id"
    }
    $arguments = @('install', '--id', $Id, '--exact', '--accept-package-agreements', '--accept-source-agreements')
    if ($Override) { $arguments += @('--override', $Override) }
    & winget @arguments
    if ($LASTEXITCODE -ne 0) { throw "Installazione non riuscita: $Id" }
}

$requirements = @(
    [pscustomobject]@{ Name = 'Node.js LTS'; Ready = (Test-Command 'node') -and (Test-Command 'npm'); Package = 'OpenJS.NodeJS.LTS'; Override = $null },
    [pscustomobject]@{ Name = 'Rust MSVC'; Ready = (Test-Command 'rustc') -and (Test-Command 'cargo'); Package = 'Rustlang.Rustup'; Override = $null },
    [pscustomobject]@{ Name = 'Visual Studio C++ Build Tools'; Ready = Test-MsvcBuildTools; Package = 'Microsoft.VisualStudio.2022.BuildTools'; Override = '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended' },
    [pscustomobject]@{ Name = 'Microsoft Edge WebView2 Runtime'; Ready = Test-WebView2; Package = 'Microsoft.EdgeWebView2Runtime'; Override = $null }
)

foreach ($requirement in $requirements) {
    $status = if ($requirement.Ready) { 'OK' } else { 'MISSING' }
    Write-Output ('[{0}] {1}' -f $status, $requirement.Name)
}

$missing = @($requirements | Where-Object { -not $_.Ready })
if ($missing.Count -gt 0 -and $CheckOnly) {
    throw "Mancano $($missing.Count) prerequisiti. Esegui make setup per installarli."
}

if ($missing.Count -gt 0) {
    if (-not $InstallMissing) { throw 'Usa -InstallMissing oppure esegui make setup.' }
    foreach ($requirement in $missing) {
        Write-Output "Installazione di $($requirement.Name)..."
        Install-WingetPackage -Id $requirement.Package -Override $requirement.Override
    }
    Write-Output 'Prerequisiti installati. Riavvia il terminale e riesegui make setup.'
    exit 2
}

if ($CheckOnly) {
    Write-Output 'Tutti i prerequisiti di sistema sono disponibili.'
    exit 0
}

Push-Location $repoRoot
try {
    $lockPath = Join-Path $repoRoot 'package-lock.json'
    $stampPath = Join-Path $repoRoot 'node_modules/.fujiview-lock.sha256'
    $lockHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $lockPath).Hash
    $installedHash = if (Test-Path -LiteralPath $stampPath) { (Get-Content -Raw -LiteralPath $stampPath).Trim() } else { '' }

    if (-not (Test-Path -LiteralPath (Join-Path $repoRoot 'node_modules')) -or $installedHash -ne $lockHash) {
        Write-Output 'Installazione delle dipendenze npm...'
        npm ci
        if ($LASTEXITCODE -ne 0) { throw 'npm ci non riuscito.' }
        Set-Content -LiteralPath $stampPath -Value $lockHash -Encoding ascii
    } else {
        Write-Output 'Dipendenze npm già aggiornate.'
    }

    Write-Output 'Setup FujiView completato.'
}
finally {
    Pop-Location
}
