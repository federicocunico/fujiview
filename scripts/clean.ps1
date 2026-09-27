[CmdletBinding()]
param([switch]$IncludeDependencies)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Split-Path -Parent $PSScriptRoot)).Path
$targets = @(
    (Join-Path $repoRoot 'dist'),
    (Join-Path $repoRoot 'src-tauri/target')
)
if ($IncludeDependencies) { $targets += (Join-Path $repoRoot 'node_modules') }

foreach ($target in $targets) {
    $fullTarget = [System.IO.Path]::GetFullPath($target)
    if (-not $fullTarget.StartsWith($repoRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Percorso di pulizia non sicuro: $fullTarget"
    }
    if (Test-Path -LiteralPath $fullTarget) {
        Write-Output "Rimozione di $fullTarget"
        Remove-Item -LiteralPath $fullTarget -Recurse -Force
    }
}

Write-Output 'Pulizia completata.'
