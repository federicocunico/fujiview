[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+([-.][0-9A-Za-z.-]+)?$')]
    [string]$Version,

    [string]$Remote = 'origin',

    [switch]$SkipLocalBuild
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot

try {
    $insideRepo = git rev-parse --is-inside-work-tree 2>$null
    if ($insideRepo -ne 'true') { throw 'La cartella non è un repository Git.' }
    if (git status --porcelain) { throw 'Il working tree deve essere pulito prima di creare una release.' }

    $branch = git branch --show-current
    if (-not $branch) { throw 'HEAD detached: passa a un branch prima della release.' }
    git remote get-url $Remote *> $null
    if ($LASTEXITCODE -ne 0) { throw "Remote Git non trovato: $Remote" }

    $tag = "v$Version"
    git rev-parse --verify --quiet "refs/tags/$tag" *> $null
    if ($LASTEXITCODE -eq 0) { throw "Il tag $tag esiste già." }

    npm version $Version --no-git-tag-version --allow-same-version
    if ($LASTEXITCODE -ne 0) { throw 'Aggiornamento package.json/package-lock.json non riuscito.' }
    Set-Content -LiteralPath (Join-Path $repoRoot 'VERSION') -Value $Version -Encoding ascii

    $tauriPath = Join-Path $repoRoot 'src-tauri/tauri.conf.json'
    $tauriText = Get-Content -Raw -LiteralPath $tauriPath
    $tauriText = [regex]::Replace($tauriText, '("version"\s*:\s*")[^"]+("\s*,)', "`$1$Version`$2", 1)
    Set-Content -LiteralPath $tauriPath -Value $tauriText -Encoding utf8

    $cargoPath = Join-Path $repoRoot 'src-tauri/Cargo.toml'
    $cargoText = Get-Content -Raw -LiteralPath $cargoPath
    $cargoText = [regex]::Replace($cargoText, '(?m)^(version\s*=\s*")[^"]+(")', "`$1$Version`$2", 1)
    Set-Content -LiteralPath $cargoPath -Value $cargoText -Encoding utf8

    & (Join-Path $PSScriptRoot 'check-version.ps1') -Tag $tag

    npm test
    if ($LASTEXITCODE -ne 0) { throw 'Test frontend falliti.' }
    cargo test --manifest-path src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Test Rust falliti.' }

    if (-not $SkipLocalBuild) {
        npm run tauri build
        if ($LASTEXITCODE -ne 0) { throw 'Build installer locale fallita.' }
    }

    git add VERSION package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
    git commit -m "chore(release): v$Version"
    if ($LASTEXITCODE -ne 0) { throw 'Commit release non riuscito.' }
    git tag -a $tag -m "FujiView $tag"
    if ($LASTEXITCODE -ne 0) { throw 'Creazione tag non riuscita.' }

    git push $Remote $branch
    if ($LASTEXITCODE -ne 0) { throw "Push del branch $branch non riuscito." }
    git push $Remote $tag
    if ($LASTEXITCODE -ne 0) { throw "Push del tag $tag non riuscito." }

    Write-Output "Release $tag inviata. GitHub Actions creerà e pubblicherà il setup NSIS."
}
finally {
    Pop-Location
}
